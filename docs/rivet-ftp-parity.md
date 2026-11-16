# Rivet FTP: feature checklist

What people expect from a mature transfer client (Cyberduck, Transmit, FileZilla), and where
Rivet FTP stands. Rivet FTP aims to work as well as those do, with its own design. It isn't a copy
of any of them. ✅ done · 🟡 partly · ⬜ not yet.

## Connecting

| Feature | | Notes |
| --- | --- | --- |
| SFTP | ✅ | Through OpenSSH: keys, agent, `~/.ssh/config`, known_hosts, jump hosts all apply |
| Saved connections (bookmarks) | ✅ | Plain file, shared with the command line and `ssh` |
| Quick connect | ✅ | |
| Categories for saved connections, folding in the sidebar | ✅ | Also `--category` on the command line |
| SSH keys: choose per connection, manage, make new | ✅ | Preferences → SSH Keys |
| Passwords and key passphrases | ✅ | Asked when needed, handed to ssh privately (askpass) |
| Remember them in the desktop keyring | ✅ | Secret Service via `secret-tool`; never in Rivet FTP's files |
| New servers trusted on first use; changed keys refused | ✅ | |
| FTP and FTPS | ⬜ | Saved already; browsing needs an FTP client (next big item) |
| WebDAV, S3, cloud drives (Dropbox, Google Drive, OneDrive…) | ⬜ | Each is its own protocol; WebDAV and S3 first |
| Reconnect after a drop | ⬜ | Open edits already pause and resume |
| Import bookmarks from other clients | ⬜ | FileZilla's sitemanager.xml is the common one |

## Browsing

| Feature | | Notes |
| --- | --- | --- |
| Two panes: this computer and the server | ✅ | |
| Stepped path bar, type a path | ✅ | |
| Sort by name, size, date | ✅ | |
| Hidden files | ✅ | |
| New folder, rename, delete | ✅ | Delete asks first |
| Properties: size, date, owner, permissions (chmod) | ✅ | Also for everything in a folder |
| Copy address of a server file | ✅ | |
| Search/filter in a folder | ⬜ | Rivet's type-to-match and filters can come over |
| Preview (pictures, video, music, code) | ✅ | Rivet's light box, shared; server files are downloaded to a private temporary folder first |
| Icon and compact views | ⬜ | Rivet's views, after the shared-widgets work |
| Duplicate on the server, symlinks, owner change | ⬜ | |

## Transfers

| Feature | | Notes |
| --- | --- | --- |
| Upload and download files and folders | ✅ | Drag between panes, double-click, buttons, keys |
| Queue with progress | 🟡 | Downloads: amount, speed, time left; uploads: time so far (sftp doesn't report upload progress in batch mode); finished: size, time taken, speed |
| If it's already there: ask, replace, skip | ✅ | "Replace if newer" and "rename" still to come |
| Retry a failed transfer | ✅ | |
| Pause and resume, resume a partial file | ⬜ | `reget` / `reput` |
| Bandwidth limit | ⬜ | `sftp -l` |
| Several transfers at once | ⬜ | One transfer session today |
| Synchronize folders (compare, then upload/download/mirror) | ⬜ | |
| Transfer log | ⬜ | |

## Editing

| Feature | | Notes |
| --- | --- | --- |
| Open a server file in an editor; saves upload | ✅ | Asks if the server's copy changed meanwhile |
| Choose the editor | ✅ | Per app, or the default for each type |

## Other

| Feature | | Notes |
| --- | --- | --- |
| Command line (list, add, ssh, sftp, keys) | ✅ | |
| Theme, accent, desktop colors, text size | ✅ | Shared with Rivet |
| Menu bar, shortcuts, keyboard control | ✅ | |
| Encrypted vaults (client-side encryption) | ⬜ | Large; later |
| Share links, CDN/cloud settings | ⬜ | Only for cloud protocols |
| Packaging: Flatpak, AUR, AppImage, launcher entry | ⬜ | |

## Suggested order for what's left

1. FTP/FTPS browsing and transfers (the protocols already in the editor).
2. Upload progress, pause/resume, several transfers at once.
3. Folder sync.
4. Reconnect, transfer log, bookmark import.
5. WebDAV and S3.
6. Rivet's views, Peek and filters in the panes (after the shared-widgets refactor).
