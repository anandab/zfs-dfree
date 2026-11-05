# zfs-dfree

Fast, zero-shell ZFS `userquota` / `groupquota` & volume helper for Samba's `dfree command`, written in Rust using direct OpenZFS `libzfs` / `libzfs_core` ioctls.

<p align="center">
  <img src="output.webp" alt="Shouting in the Datacenter" /><br>
  <a href="https://www.youtube.com/watch?v=tDacjrSCeq4">Original Video</a>
</p>

## Why zfs-dfree?

### Apple Time Machine ZFS and Linux Quotas

When sharing a directory over Samba as an Apple Time Machine destination using `vfs_fruit`:

```ini
fruit:time machine max size = 4T
```

If you do not use ZFS quotas you can specify how much space that directory can use using 'time machine max size'. Samba attempts to approximate available space by opening the `.sparsebundle` and executing a directory crawl (`readdir`) over every band file in `bands/`. For multi-terabyte backups, this means inspecting 100,000+ files on every single disk check. After months of backups, disk queries begin taking 10 to 60+ seconds of synchronous I/O, causing Time Machine to freeze on _"Preparing backup..."_ or fail with timeouts. This tool will allow you to have multiple users on a time machine share and respect the quota utilization.

## Building & Installing

### Requirements

- Linux with OpenZFS (`libzfs4linux` / `zfsutils-linux`). No `-dev` packages required.
- Rust toolchain (`cargo`).

### Build & Install

```bash
make
sudo make install
```

This builds an optimized release binary with Link-Time Optimization (LTO) and installs it to `/usr/local/bin/zfs-dfree`.

## Samba Configuration (`/etc/samba/smb.conf`)

### Option A: Apple Time Machine Share

```ini
[timemachine]
comment = Time Machine Backups
path = /tank/backup
valid users = alice bob
writable = yes
browseable = no
create mask = 0600
directory mask = 0700
vfs objects = catia fruit streams_xattr
fruit:time machine = yes
dfree command = /usr/local/bin/zfs-dfree
dfree cache time = 60
```

> **Tip:** Do **not** set `fruit:time machine max size`. Instead, configure quotas directly in ZFS:
>
> ```bash
>  sudo zfs set userquota@alice=2T tank
>  sudo zfs set userquota@bob=4T tank
> ```

### Option B: Multi-User / Home Directories

```ini
[homes]
comment = User Home Directories
path = /tank/home/%U
valid users = %S
writable = yes
browseable = no
dfree command = /usr/local/bin/zfs-dfree
dfree cache time = 60
```

### Option C: Department / Team Share (Group Quota)

```ini
[marketing]
comment = Marketing Team Share
path = /tank/shared/marketing
valid users = @marketing
writable = yes
dfree command = /usr/local/bin/zfs-dfree
dfree cache time = 60
```

```bash
sudo zfs set groupquota@marketing=10T tank
```

Feel free to use `dfree cache time` to what

## CLI Usage

### Automatic (Samba Mode / Zero Arguments)

Inside any ZFS mount directory:

```bash
cd /tank/backup
zfs-dfree
```

Automatically resolves the current user (`geteuid()`) and the backing dataset (`zfs_path_to_zhandle`).

### Manual Inspection

```bash
# Query explicit user on dataset or path:
zfs-dfree timemachine tank
zfs-dfree timemachine /tank/backup
```

Output format expected by Samba (`<total_1k_blocks> <avail_1k_blocks> 1024`):

```text
4294967296 2991133628 1024
```

## Performance Benchmark

Benchmarked using `hyperfine` (500 runs against a live ZFS pool on Linux 5.15):

```text
Benchmark: zfs-dfree timemachine tank
Time (mean ± σ):       2.9 ms ± 0.1 ms
User CPU Time:         1.5 ms
System Kernel Time:    1.4 ms
```

## NOTES

This is a Linux only issue.
OpenZFS on Linux does not yet support quotactl_fd(2). When samba checks to see how much disk space is available it uses the legacy quota syscall. One would need to wire OpenZFS with quotactl_fd and Samba to use quotactl_fd. This fix happens in userspace and runs every 60 seconds. On my system it takes about 3ms to execute.

## Licensed

SPDX-License-Identifier: `Apache-2.0`
