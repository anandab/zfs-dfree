// SPDX-License-Identifier: Apache-2.0
//! zfs-dfree - Fast ZFS userquota/groupquota & volume helper for Samba dfree.

use std::env;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::process;

type Hdl = *mut c_void;
const ZFS_TYPE_FILESYSTEM: c_int = 1 << 0;

struct Zfs {
    _core: libloading::Library,
    _zfs: libloading::Library,
    core_fini: unsafe extern "C" fn(),
    lib_fini: unsafe extern "C" fn(Hdl),
    path_to_zhdl: unsafe extern "C" fn(Hdl, *const c_char, c_int) -> Hdl,
    zfs_close: unsafe extern "C" fn(Hdl),
    get_quota: unsafe extern "C" fn(Hdl, *const c_char, *mut u64) -> c_int,
    name_to_prop: unsafe extern "C" fn(*const c_char) -> c_int,
    prop_get_int: unsafe extern "C" fn(Hdl, c_int) -> u64,
    hdl: Hdl,
}

impl Zfs {
    fn load() -> Result<Self, String> {
        let load = |paths: &[&str]| {
            paths
                .iter()
                .find_map(|p| unsafe { libloading::Library::new(p) }.ok())
        };

        let core = load(&[
            "/usr/lib/x86_64-linux-gnu/libzfs_core.so.3",
            "libzfs_core.so.3",
            "libzfs_core.so",
        ])
        .ok_or("Failed to load libzfs_core.")?;

        let zfs = load(&[
            "/usr/lib/x86_64-linux-gnu/libzfs.so.4",
            "libzfs.so.4",
            "libzfs.so",
        ])
        .ok_or("Failed to load libzfs.")?;

        unsafe {
            macro_rules! sym {
                ($lib:expr, $name:literal) => {
                    *$lib.get($name).map_err(|e| e.to_string())?
                };
            }

            let core_init: extern "C" fn() -> c_int =
                sym!(core, b"libzfs_core_init\0");
            let core_fini: extern "C" fn() = sym!(core, b"libzfs_core_fini\0");
            let lib_init: extern "C" fn() -> Hdl = sym!(zfs, b"libzfs_init\0");
            let lib_fini: extern "C" fn(Hdl) = sym!(zfs, b"libzfs_fini\0");
            let path_to_zhdl: extern "C" fn(Hdl, *const c_char, c_int) -> Hdl =
                sym!(zfs, b"zfs_path_to_zhandle\0");
            let zfs_close: extern "C" fn(Hdl) = sym!(zfs, b"zfs_close\0");
            let get_quota: extern "C" fn(
                Hdl,
                *const c_char,
                *mut u64,
            ) -> c_int = sym!(zfs, b"zfs_prop_get_userquota_int\0");
            let name_to_prop: extern "C" fn(*const c_char) -> c_int =
                sym!(zfs, b"zfs_name_to_prop\0");
            let prop_get_int: extern "C" fn(Hdl, c_int) -> u64 =
                sym!(zfs, b"zfs_prop_get_int\0");

            if core_init() != 0 {
                return Err("Failed to initialize libzfs_core.".into());
            }
            let hdl = lib_init();
            if hdl.is_null() {
                core_fini();
                return Err("Failed to initialize libzfs.".into());
            }

            Ok(Self {
                _core: core,
                _zfs: zfs,
                core_fini,
                lib_fini,
                path_to_zhdl,
                zfs_close,
                get_quota,
                name_to_prop,
                prop_get_int,
                hdl,
            })
        }
    }

    fn open_target(&self, target: &str) -> Result<Hdl, String> {
        let path = std::fs::canonicalize(target).map_or_else(
            |_| target.to_string(),
            |p| p.to_string_lossy().into(),
        );
        let c_target = CString::new(path).map_err(|e| e.to_string())?;

        let zhp = unsafe {
            (self.path_to_zhdl)(
                self.hdl,
                c_target.as_ptr(),
                ZFS_TYPE_FILESYSTEM,
            )
        };

        if zhp.is_null() {
            let msg = if target == "." || target.starts_with('/') {
                "Current directory is not on a mounted ZFS dataset.".into()
            } else {
                format!("Failed to open ZFS dataset '{target}'.")
            };
            Err(msg)
        } else {
            Ok(zhp)
        }
    }

    fn query_quota(&self, zhp: Hdl, user: &str) -> (u64, u64) {
        let read = |p: &str| -> Option<u64> {
            let mut v = 0u64;
            let rc = unsafe {
                (self.get_quota)(zhp, CString::new(p).ok()?.as_ptr(), &mut v)
            };
            (rc == 0 && v > 0).then_some(v)
        };

        let mut quota = read(&format!("userquota@{user}")).unwrap_or(0);
        let mut used = read(&format!("userused@{user}")).unwrap_or(0);

        if quota == 0 {
            if let Some(grp) = get_primary_group(user) {
                quota = read(&format!("groupquota@{grp}")).unwrap_or(0);
                used = read(&format!("groupused@{grp}")).unwrap_or(0);
            }
        }

        let p_avail = unsafe { (self.name_to_prop)(c"available".as_ptr()) };
        let vol_avail = unsafe { (self.prop_get_int)(zhp, p_avail) };

        let (total, avail) = if quota > 0 {
            (quota, quota.saturating_sub(used).min(vol_avail))
        } else {
            let p_used = unsafe { (self.name_to_prop)(c"used".as_ptr()) };
            let p_quota = unsafe { (self.name_to_prop)(c"quota".as_ptr()) };
            let vol_used = unsafe { (self.prop_get_int)(zhp, p_used) };
            let vol_quota = unsafe { (self.prop_get_int)(zhp, p_quota) };

            let t =
                if vol_quota > 0 { vol_quota } else { vol_used + vol_avail };
            let a = if vol_quota > 0 {
                vol_quota.saturating_sub(vol_used).min(vol_avail)
            } else {
                vol_avail
            };
            (t, a)
        };

        unsafe { (self.zfs_close)(zhp) };
        (total, avail)
    }
}

impl Drop for Zfs {
    fn drop(&mut self) {
        unsafe {
            (self.lib_fini)(self.hdl);
            (self.core_fini)();
        }
    }
}

fn get_current_user() -> Result<String, String> {
    unsafe {
        let uid = libc::geteuid();
        let pw = libc::getpwuid(uid);
        if pw.is_null() || (*pw).pw_name.is_null() {
            return Err(format!("Failed to resolve username for UID {uid}."));
        }
        Ok(CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned())
    }
}

fn get_primary_group(user: &str) -> Option<String> {
    unsafe {
        let pw = libc::getpwnam(CString::new(user).ok()?.as_ptr());
        if pw.is_null() {
            return None;
        }
        let gr = libc::getgrgid((*pw).pw_gid);
        (!gr.is_null() && !(*gr).gr_name.is_null()).then(|| {
            CStr::from_ptr((*gr).gr_name).to_string_lossy().into_owned()
        })
    }
}

fn err_exit(msg: &str) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}

fn print_help(prog: &str) {
    println!(
        "Usage: {prog} [username] [pool_or_path]\n\
         Fast ZFS user/group quota & volume helper for Samba dfree command.\n\
         Queries ZFS quotas directly via /dev/zfs ioctl without shell overhead.\n\
         Samba mode: running as '.' or no args resolves geteuid() & current dataset.\n\
         Output format: <total_1k_blocks> <avail_1k_blocks> 1024"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let prog = args.first().map(|s| s.as_str()).unwrap_or("zfs-dfree");

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help(prog);
        return;
    }

    let (user, target) = match &args[1..] {
        [] => (get_current_user().unwrap_or_else(|e| err_exit(&e)), "."),
        [p] if p == "." || Path::new(p).is_dir() => {
            (get_current_user().unwrap_or_else(|e| err_exit(&e)), p.as_str())
        }
        [u] => err_exit(&format!(
            "Missing required parameter: <pool_name> (got username '{u}').\n\
             Run '{prog} --help' for usage information."
        )),
        [u, p, ..] => (u.clone(), p.as_str()),
    };

    let zfs = Zfs::load().unwrap_or_else(|e| err_exit(&e));
    let zhp = zfs.open_target(target).unwrap_or_else(|e| err_exit(&e));
    let (total, avail) = zfs.query_quota(zhp, &user);

    println!("{} {} 1024", total / 1024, avail / 1024);
}
