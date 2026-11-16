# Rivet FTP

A dual-pane file-transfer app for SFTP, FTP and FTPS, built on Rivet's core: this computer on
one side, the server on the other, and a transfer queue underneath. It's in development
([plan and keys](../../docs/rivet-ftp.md)). SFTP works; FTP and FTPS come next.

```sh
cargo build -p rivet-ftp
./target/debug/rivet-ftp                 # the window
./target/debug/rivet-ftp open work-site  # the window, connected
./target/debug/rivet-ftp --help          # the command line
```

SFTP goes through your system's own `sftp` program (OpenSSH), so your keys, ssh agent,
`~/.ssh/config` and known_hosts all apply. Drag files between the panes, double-click a file to
send it across, or use Upload and Download.

## Saved connections

```sh
rivet-ftp add "Work site" sftp://me@example.com:2222/var/www --key ~/.ssh/id_ed25519 --local ~/Sites/work
rivet-ftp add Pi pi@192.168.1.4          # user@server alone means SFTP
rivet-ftp add Mirror ftps://files.example.org/pub --category Clients   # a group in the sidebar
rivet-ftp list
rivet-ftp show work-site                 # by name, or its short form
rivet-ftp remove Pi
```

Addresses look like `sftp://user@server:port/folder`, `ftp://…` or `ftps://…`. Passwords are
never saved in the connections file. The window will keep them in your keyring, and SSH uses
your keys or ssh agent.

## Editing files on the server

Right-click a file on the server and choose **Open in Editor** (or press F4), or **Open With** to
pick an app. Rivet FTP downloads it to a private folder that only you can read, opens it, and
sends every save back to the server. If someone changed the server's copy since you opened it,
it asks before replacing theirs. The files open for editing are listed above the transfers;
**Stop** ends it and deletes the copy. Preferences → Transfers chooses the editor.

## SSH keys

```sh
rivet-ftp keys                           # your keys: type, passphrase, ssh agent, which connections use them
rivet-ftp add Pi pi@192.168.1.4 --key id_ed25519   # a key's short name, or a file
```

In the window, Preferences → SSH Keys lists the keys in `~/.ssh`, makes new ones (Ed25519 or
RSA 4096, with an optional passphrase), copies a public key, and adds one to a server
(`ssh-copy-id` in a terminal). Each connection's settings pick its key. Private keys never leave
`~/.ssh`. A key with a passphrase needs to be in your ssh agent (`ssh-add`) before connecting.

## Using them from a terminal

```sh
rivet-ftp ssh work-site                  # log in, starting in /var/www
rivet-ftp ssh work-site git status       # run a command there
rivet-ftp sftp work-site                 # the sftp command line, in /var/www
rivet-ftp url work-site                  # sftp://me@example.com:2222/var/www
rivet-ftp ssh work-site --print          # show the ssh command instead of running it
```

Every save also writes `~/.config/rivet-ftp/ssh_config`. Add this line near the top of
`~/.ssh/config`, and `ssh work-site`, `scp`, `rsync` and `git` will reach saved connections by name:

```
Include ~/.config/rivet-ftp/ssh_config
```

`rivet-ftp ssh-config` shows this again, with the right path.

## The file

`~/.config/rivet-ftp/connections.conf` is plain text, and only you can read it. You can edit it by hand:

```ini
[Work site]
protocol=sftp
host=example.com
port=2222
user=me
key=~/.ssh/id_ed25519
remote=/var/www
local=~/Sites/work
```
