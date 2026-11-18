#!/usr/bin/env python3
# A fake StatusNotifierItem with a dbusmenu, for testing the tray. Logs calls to argv[1].
import sys
from gi.repository import Gio, GLib

LOG = open(sys.argv[1], "a", buffering=1)
NAME = "org.kde.StatusNotifierItem-%d-1" % __import__("os").getpid()

ITEM_XML = """<node><interface name="org.kde.StatusNotifierItem">
<property name="Id" type="s" access="read"/><property name="Title" type="s" access="read"/>
<property name="Status" type="s" access="read"/><property name="IconName" type="s" access="read"/>
<property name="IconPixmap" type="a(iiay)" access="read"/>
<property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
<property name="Menu" type="o" access="read"/><property name="ItemIsMenu" type="b" access="read"/>
<method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
<signal name="NewIcon"/>
</interface></node>"""
MENU_XML = """<node><interface name="com.canonical.dbusmenu">
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/>
<arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
</interface></node>"""

state = {"title": "Fake App"}
px = [GLib.Variant("(iiay)", (2, 2, bytes([255, 255, 0, 0] * 4)))]

def item_prop(conn, sender, path, iface, prop):
    return {
        "Id": GLib.Variant("s", "fake"), "Title": GLib.Variant("s", state["title"]),
        "Status": GLib.Variant("s", "Active"), "IconName": GLib.Variant("s", "no-such-icon-xyz"),
        "IconPixmap": GLib.Variant("a(iiay)", [(2, 2, bytes([255, 255, 0, 0] * 4))]),
        "ToolTip": GLib.Variant("(sa(iiay)ss)", ("", [], "Fake App", "Some <b>text</b>")),
        "Menu": GLib.Variant("o", "/MenuBar"), "ItemIsMenu": GLib.Variant("b", False),
    }[prop]

def item_call(conn, sender, path, iface, method, params, inv):
    LOG.write("item %s %s\n" % (method, params.unpack()))
    inv.return_value(None)
    if method == "SecondaryActivate":
        state["title"] = "Renamed"
        conn.emit_signal(None, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "NewIcon", None)

def row(i, label, kids=(), **extra):
    props = {"label": GLib.Variant("s", label)}
    props.update(extra)
    return GLib.Variant("(ia{sv}av)", (i, props, [GLib.Variant("v", k) for k in kids]))

def menu_call(conn, sender, path, iface, method, params, inv):
    LOG.write("menu %s %s\n" % (method, params.unpack()))
    if method == "GetLayout":
        layout = (0, {}, [
            GLib.Variant("v", row(1, "_Open")),
            GLib.Variant("v", row(2, "", type=GLib.Variant("s", "separator"))),
            GLib.Variant("v", row(3, "Sub", [row(31, "Inner")], **{"children-display": GLib.Variant("s", "submenu")})),
            GLib.Variant("v", row(4, "Hidden", visible=GLib.Variant("b", False))),
            GLib.Variant("v", row(5, "Check", **{"toggle-type": GLib.Variant("s", "checkmark"), "toggle-state": GLib.Variant("i", 1)})),
        ])
        inv.return_value(GLib.Variant("(u(ia{sv}av))", (1, layout)))
    elif method == "AboutToShow":
        inv.return_value(GLib.Variant("(b)", (False,)))
    else:
        inv.return_value(None)

def on_bus(conn, name):
    conn.register_object("/StatusNotifierItem", Gio.DBusNodeInfo.new_for_xml(ITEM_XML).interfaces[0], item_call, item_prop, None)
    conn.register_object("/MenuBar", Gio.DBusNodeInfo.new_for_xml(MENU_XML).interfaces[0], menu_call, None, None)

def on_name(conn, name):
    def register(*_):
        try:
            conn.call_sync("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher",
                           "RegisterStatusNotifierItem", GLib.Variant("(s)", (NAME,)), None, 0, -1, None)
            LOG.write("registered\n")
            return False
        except Exception as e:
            return True  # try again until the watcher is there
    GLib.timeout_add(100, register)

Gio.bus_own_name(Gio.BusType.SESSION, NAME, 0, on_bus, on_name, None)
GLib.MainLoop().run()
