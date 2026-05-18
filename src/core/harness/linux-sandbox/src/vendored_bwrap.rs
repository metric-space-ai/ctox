//! Build-time bubblewrap entrypoint.
//!
//! On Linux targets, the build script compiles bubblewrap's C sources and
//! exposes a `bwrap_main` symbol that we can call via FFI.

#[cfg(vendored_bwrap_available)]
mod imp {
    use std::ffi::CString;
    use std::fs::File;
    use std::os::raw::c_char;

    unsafe extern "C" {
        fn bwrap_main(argc: libc::c_int, argv: *const *const c_char) -> libc::c_int;
    }

    fn argv_to_cstrings(argv: &[String]) -> Vec<CString> {
        let mut cstrings: Vec<CString> = Vec::with_capacity(argv.len());
        for arg in argv {
            match CString::new(arg.as_str()) {
                Ok(value) => cstrings.push(value),
                Err(err) => panic!("failed to convert argv to CString: {err}"),
            }
        }
        cstrings
    }

    /// Run the build-time bubblewrap `main` function and return its exit code.
    ///
    /// On success, bubblewrap will `execve` into the target program and this
    /// function will never return. A return value therefore implies failure.
    pub(crate) fn run_vendored_bwrap_main(
        argv: &[String],
        _preserved_files: &[File],
    ) -> libc::c_int {
        let cstrings = argv_to_cstrings(argv);

        let mut argv_ptrs: Vec<*const c_char> = cstrings.iter().map(|arg| arg.as_ptr()).collect();
        argv_ptrs.push(std::ptr::null());

        // SAFETY: We provide a null-terminated argv vector whose pointers
        // remain valid for the duration of the call.
        unsafe { bwrap_main(cstrings.len() as libc::c_int, argv_ptrs.as_ptr()) }
    }

    /// Execute the build-time bubblewrap `main` function with the given argv.
    pub(crate) fn exec_vendored_bwrap(argv: Vec<String>, preserved_files: Vec<File>) -> ! {
        let exit_code = run_vendored_bwrap_main(&argv, &preserved_files);
        std::process::exit(exit_code);
    }
}

#[cfg(not(vendored_bwrap_available))]
mod imp {
    use std::fs::File;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    fn system_bwrap_binary() -> String {
        for candidate in [
            "/usr/bin/bwrap",
            "/bin/bwrap",
            "/usr/bin/bubblewrap",
            "/bin/bubblewrap",
        ] {
            if std::path::Path::new(candidate).exists() {
                return candidate.to_string();
            }
        }
        "bwrap".to_string()
    }

    /// Execute a system bubblewrap binary and return its exit code.
    pub(crate) fn run_vendored_bwrap_main(
        argv: &[String],
        _preserved_files: &[File],
    ) -> libc::c_int {
        let binary = system_bwrap_binary();
        let mut command = Command::new(binary);
        if argv.len() > 1 {
            command.args(&argv[1..]);
        }
        match command.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(err) => panic!("failed to execute system bubblewrap fallback: {err}"),
        }
    }

    /// Exec a system bubblewrap binary when vendored sources are unavailable.
    pub(crate) fn exec_vendored_bwrap(argv: Vec<String>, _preserved_files: Vec<File>) -> ! {
        let binary = system_bwrap_binary();
        let err = Command::new(binary).args(argv.into_iter().skip(1)).exec();
        panic!("failed to exec system bubblewrap fallback: {err}");
    }
}

pub(crate) use imp::exec_vendored_bwrap;
pub(crate) use imp::run_vendored_bwrap_main;
