# Journey J1: two real computers

J1 runs on **two real machines**, both on this one Mac mini:

- **Computer A (controller)**: the Mac. The runner runs here, drives the Mac's
  pointer and keyboard with `osascript` (CoreGraphics events at the HID tap, see
  `input.rs`), and runs the local `mouser` CLI.
- **Computer B**: a **Windows 11 ARM64 VM in Parallels** on the same Mac. The
  runner reaches it over SSH (Windows OpenSSH Server) and runs `mouser.exe`
  there. Everything sent is an encoded PowerShell script, so the OpenSSH default
  shell does not matter.

Run it only with `cargo xtask gate --e2e j1 --lane convergence` (or
`cargo test -p mouser-cli --features e2e --test e2e_j1`). It posts real input on
the Mac: do not touch the mouse or keyboard while it runs (about 3 minutes).
Quit ShareMouse first.

## Owner checklist

### Windows VM (once)

1. Parallels: set the VM network to **Bridged Network (Default Adapter)**.
   Shared/NAT hides the VM from Mouser's LAN discovery and J1 would fail to find
   the desk. Keep the VM's window from capturing the Mac's mouse and keyboard
   (run it in the background; do not click into it while J1 runs).
2. Log in to Windows as the user that will own the test and keep that session
   logged in (no lock screen sign-out). In an **elevated** PowerShell in the VM:

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
New-NetFirewallRule -Name sshd-j1 -DisplayName 'OpenSSH (J1)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
# Mouser's own discovery/sync ports are opened by its installer prompt; allow the
# prompt "Allow Mouser on private networks" the first time it appears.
Get-NetConnectionProfile   # the Bridged adapter should be Private; if Public:
# Set-NetConnectionProfile -InterfaceAlias 'Ethernet' -NetworkCategory Private
```

3. Install the Mac's public key for that user. On the Mac:

```sh
ssh-keygen -t ed25519 -f ~/.ssh/mouser_j1 -N ''
cat ~/.ssh/mouser_j1.pub
```

   In the VM, for a **standard** user (replace the key text):

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.ssh" | Out-Null
Add-Content "$env:USERPROFILE\.ssh\authorized_keys" 'ssh-ed25519 AAAA... you@mac'
```

   For an **Administrator** user, Windows OpenSSH reads a different file
   (elevated PowerShell):

```powershell
Add-Content C:\ProgramData\ssh\administrators_authorized_keys 'ssh-ed25519 AAAA... you@mac'
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F'
```

4. Install Mouser (see "Install" below), launch it once by hand, and grant the
   prompts: the firewall prompt (private networks) and any input permission
   prompts. Then in Notepad Settings turn **off** "Open content from previous
   session", and set "Opening files" to "Open in a new window" if you can.
   **Close every Notepad on B before a run**: Windows 11 Notepad is
   single-process with tabs, so J1 refuses to start when any Notepad is running
   (it would hand the file to that Notepad as a tab). J1 never kills Notepad; it
   closes only the window it opened, by handle, after the file is saved.
5. Find the installed paths and note them (the defaults the runner assumes are
   `%LOCALAPPDATA%\Mouser\mouser.exe` and `%LOCALAPPDATA%\Mouser\mouser-app.exe`):

```powershell
Get-ChildItem "$env:LOCALAPPDATA\Mouser" -Filter *.exe
```

6. Test from the Mac: `ssh -i ~/.ssh/mouser_j1 USER@VMHOST 'echo ok'`.

### Mac (each session)

1. Install the app (below), launch it once, grant **Accessibility** and **Input
   Monitoring** to Mouser, and make the CLI reachable:

```sh
sudo ln -sf /Applications/Mouser.app/Contents/MacOS/mouser /usr/local/bin/mouser
mouser --version
```

2. Grant **Accessibility** to the terminal (or app) that runs the test; it posts
   the events through `osascript`.
3. Export the settings:

```sh
export MOUSER_J1_PEER='USER@VMHOST'          # ssh target of the Windows VM
export MOUSER_J1_KEY="$HOME/.ssh/mouser_j1"  # private key from step 3
export MOUSER_J1_VM='Windows 11'             # name from `prlctl list -a`
# Only if the defaults are wrong (see step 5 above):
# export MOUSER_J1_PEER_CLI='C:\Users\USER\AppData\Local\Mouser\mouser.exe'
# export MOUSER_J1_PEER_APP='C:\Users\USER\AppData\Local\Mouser\mouser-app.exe'
# export MOUSER_J1_PEER_PROC='mouser-app'    # process name without .exe
# export MOUSER_J1_LOCAL_CLI=mouser
prlctl list -a    # the VM must be running, with the Windows user logged in
```

4. Quit any other Mouser or ShareMouse running on the Mac, then run
   `cargo xtask gate --e2e j1 --lane convergence`.

## Install the release-workflow build

- **Mac**: download the universal `.dmg` from the release (built by
  `.github/workflows/release.yml`), drag Mouser to Applications.
- **Windows (recommended)**: the release's x64 `*-setup.exe` (NSIS, per-user
  install). It runs under Windows 11 ARM64's x64 emulation, and it is exactly
  the artifact users get, which is what J1 is meant to test. Copy it into the VM
  and run it.
- **Windows (fallback)**: an ARM64 build from a checkout inside the VM
  (`rustup target add aarch64-pc-windows-msvc`, then build the app and the
  `mouser` sidecar for that target). Use it only if the emulated x64 build cannot
  install its input hooks. Nothing in the release workflow produces it.

## Why `prlctl suspend` stands in for sleep

Story 3 needs a computer to go away and come back. A Windows sleep inside the VM
cannot be woken from the runner (no wake-on-LAN for a VM, and SSH dies with the
network). `prlctl suspend <vm>` / `prlctl resume <vm>` freezes the VM: its
network disappears, Mouser on both sides notices the peer is gone, its processes
stop, then everything returns. That is what Mouser observes on a real sleep and
wake, and the runner can drive both halves from the Mac.

## How text on B is verified

The runner starts Notepad on B inside the logged-in desktop session through a
scheduled task (`schtasks /it`), because an SSH session has no desktop. The file
is `C:\Users\Public\mouser-j1-note.txt`. Notepad is maximised; after the pointer crosses, the
runner moves it into the middle of B's display with the shared mouse and clicks,
then asks B's desktop session for the foreground window and fails unless it is
`mouser-j1-note.txt`. Typing on the Mac then goes to Notepad; Command-S (left
Command as a flags-changed event carrying its left-key bit, then S, posted at
the HID tap like the letters; never System Events `keystroke`, which posts at
the session tap after Mouser's HID tap and so bypasses Mouser) arrives as
Ctrl-S and saves; the runner first fails if Secure Input is on on the Mac
(it hides keys from every event tap, Mouser's included); the runner reads the
file over SSH and compares it exactly. The CLI cannot observe received keys, so
the file is the observation. The same mechanism starts the Mouser app on B.

## Known limits

- Story 1 scenario 9 is exercised as an app restart on B (force-stop and
  relaunch), not a Windows reboot and login.
- Story 1 scenario 4 uses two single-computer desks (B first holds "Lab bench");
  leakage of input or clipboard between desks needs a third computer.
