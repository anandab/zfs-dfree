# zfs-dfree

Fast, zero-shell ZFS `userquota` / `groupquota` & volume helper for Samba's `dfree command`, written in Rust using direct OpenZFS `libzfs` / `libzfs_core` ioctls.

## Why zfs-dfree?

### 1. The Samba + ZFS Problem
When sharing a ZFS filesystem over Samba, Samba cannot automatically report ZFS `userquota@` or `groupquota@` limits to connected clients because OpenZFS does not implement the legacy Linux kernel `quotactl(2)` syscall. As a result, Windows Explorer, macOS Finder, and Linux clients report the full physical pool capacity instead of individual user quotas.

### 2. The Apple Time Machine Problem
When sharing a directory over Samba as an Apple Time Machine destination using `vfs_fruit`:
```ini
fruit:time machine max size = 4T
```
Samba attempts to approximate available space by opening the `.sparsebundle` and executing a directory crawl (`readdir`) over every band file in `bands/`. For multi-terabyte backups, this means inspecting 100,000+ files on every single disk check. After months of backups, disk queries begin taking 10 to 60+ seconds of synchronous I/O, causing Time Machine to freeze on *"Preparing backup..."* or fail with timeouts.

### 3. The Solution
`zfs-dfree` solves both problems:
* **Zero Shell Overhead:** Queries `/dev/zfs` directly via `libzfs` in **$< 3\text{ ms}$** without spawning `/bin/sh` or `/sbin/zfs`.
* **Universal Quota Resolution:**
  1. Checks `userquota@<username>` for the connected client.
  2. Falls back to `groupquota@<group>` if user quota is unset.
  3. Falls back to total volume capacity & free space if no quotas are configured.
* **Underflow & Overcommit Protection:** Available space is clamped to zero on over-quota conditions and bounded by physical pool free space.
* **Zero-Configuration Samba Integration:** Automatically detects the connected user via `geteuid()` and resolves the backing ZFS dataset using `zfs_path_to_zhandle`.

## Building & Installing

### Requirements
* Linux with OpenZFS (`libzfs4linux` / `zfsutils-linux`). No `-dev` packages required.
* Rust toolchain (`cargo`).

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
> ```bash
> sudo zfs set userquota@alice=2T tank
> sudo zfs set userquota@bob=4T tank
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

## License

SPDX-License-Identifier: `Apache-2.0`
