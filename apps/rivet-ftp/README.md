# Rivet FTP

A dual-pane file-transfer app for SFTP, FTP and FTPS, built on Rivet's core. The window is still
in development ([plan](../../docs/rivet-ftp.md)). Saved connections already work from the command
line, and plain `ssh` can use them too.

```sh
cargo build -p rivet-ftp
./target/debug/rivet-ftp --help
```

## Saved connections

```sh
rivet-ftp add "Work site" sftp://me@example.com:2222/var/www --key ~/.ssh/id_ed25519 --local ~/Sites/work
rivet-ftp add Pi pi@192.168.1.4          # user@server alone means SFTP
rivet-ftp add Mirror ftps://files.example.org/pub
rivet-ftp list
rivet-ftp show work-site                 # by name, or its short form
rivet-ftp remove Pi
```

Addresses look like `sftp://user@server:port/folder`, `ftp://…` or `ftps://…`. Passwords are
never saved in the connections file. The window will keep them in your keyring, and SSH uses
your keys or ssh agent.

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
