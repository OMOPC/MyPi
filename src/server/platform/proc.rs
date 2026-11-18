// One command's process tree: a kernel container with a leader, taken down as a whole.

use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a stopped tree gets between the polite request and the fatal one.
pub fn grace() -> Duration {
    Duration::from_secs(2)
}

/// A tree: the leader, its streams, and the platform handle that owns every process it starts.
#[derive(Clone)]
pub struct Process(Arc<Inner>);

struct Inner {
    pid: u32,
    child: Mutex<Child>,
    io: Mutex<Io>,
    group: sys::Group,
}

struct Io {
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process").field("pid", &self.0.pid).finish()
    }
}

impl Process {
    /// Start a command as the leader of a fresh tree; the platform's own handle owns the tree.
    pub fn spawn(cmd: &mut Command) -> std::io::Result<Process> {
        sys::prepare(cmd);
        let mut child = cmd.spawn()?;
        let pid = child.id();
        let io = Io {
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
        };
        let group = sys::Group::attach(pid);
        Ok(Process(Arc::new(Inner {
            pid,
            child: Mutex::new(child),
            io: Mutex::new(io),
            group,
        })))
    }

    /// The leader's pid; the tree is the unit, this is only its name in a log.
    pub fn id(&self) -> u32 {
        self.0.pid
    }

    pub fn take_stdin(&self) -> Option<ChildStdin> {
        self.0.lock_io().stdin.take()
    }

    pub fn take_stdout(&self) -> Option<ChildStdout> {
        self.0.lock_io().stdout.take()
    }

    pub fn take_stderr(&self) -> Option<ChildStderr> {
        self.0.lock_io().stderr.take()
    }

    /// Ask the tree to stop. On windows there is no polite signal, so this is a kill.
    pub fn interrupt(&self) {
        self.0.group.interrupt();
    }

    /// Take the tree down now.
    pub fn kill(&self) {
        self.0.group.kill();
    }

    pub fn try_wait(&self) -> std::io::Result<Option<ExitStatus>> {
        self.0.lock_child().try_wait()
    }

    pub fn wait(&self) -> std::io::Result<ExitStatus> {
        self.0.lock_child().wait()
    }
}

impl Inner {
    fn lock_child(&self) -> std::sync::MutexGuard<'_, Child> {
        self.child.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_io(&self) -> std::sync::MutexGuard<'_, Io> {
        self.io.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(unix)]
mod sys {
    use std::io;
    use std::process::Command;

    unsafe extern "C" {
        fn setsid() -> i32;
        fn kill(pid: i32, sig: i32) -> i32;
    }

    const SIGTERM: i32 = 15;
    const SIGKILL: i32 = 9;

    /// A new session: no controlling terminal, and the child leads its own group.
    pub fn prepare(cmd: &mut Command) {
        use std::os::unix::process::CommandExt as _;
        // SAFETY: `setsid` is one syscall, and the child has not execed yet.
        unsafe {
            cmd.pre_exec(|| {
                if setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                if linux::die_with_parent() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    pub struct Group {
        pgid: u32,
    }

    impl Group {
        pub fn attach(pid: u32) -> Group {
            Group { pgid: pid }
        }

        pub fn interrupt(&self) {
            // SAFETY: a plain syscall; a group that has already vanished is the normal case.
            unsafe {
                kill(-(self.pgid as i32), SIGTERM);
            }
        }

        pub fn kill(&self) {
            // SAFETY: a plain syscall; a group that has already vanished is the normal case.
            unsafe {
                kill(-(self.pgid as i32), SIGKILL);
            }
        }
    }

    impl Drop for Group {
        fn drop(&mut self) {
            self.kill();
        }
    }

    #[cfg(target_os = "linux")]
    mod linux {
        const PR_SET_PDEATHSIG: i32 = 1;
        const SIGKILL: i32 = 9;

        unsafe extern "C" {
            fn prctl(option: i32, ...) -> i32;
        }

        /// Ask the kernel to kill the leader once the forking thread ends.
        pub fn die_with_parent() -> i32 {
            // SAFETY: one syscall with constant arguments.
            unsafe { prctl(PR_SET_PDEATHSIG, SIGKILL as u64, 0u64, 0u64, 0u64) }
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    use std::mem::{MaybeUninit, size_of};
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> isize;
        fn SetInformationJobObject(job: isize, class: u32, info: *const c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
        fn TerminateJobObject(job: isize, exit_code: u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    /// No console window for a child started from a process that has none.
    pub fn prepare(cmd: &mut Command) {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    pub struct Group {
        pid: u32,
        job: isize,
    }

    // SAFETY: a job handle is a plain kernel handle; using it from another thread is a documented Win32 operation.
    unsafe impl Send for Group {}
    unsafe impl Sync for Group {}

    impl Group {
        /// Put the new leader in a job that kills its members when the last handle closes.
        pub fn attach(pid: u32) -> Group {
            // SAFETY: every call is checked; a null handle means "fall back to taskkill".
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job == 0 {
                    return Group { pid, job: 0 };
                }
                let mut info = MaybeUninit::<JobObjectExtendedLimitInformation>::zeroed();
                (*info.as_mut_ptr()).basic_limit_information.limit_flags =
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let info = info.assume_init();
                let set = SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &info as *const _ as *const c_void,
                    size_of::<JobObjectExtendedLimitInformation>() as u32,
                );
                let process = if set == 0 {
                    0
                } else {
                    OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid)
                };
                if process == 0 || AssignProcessToJobObject(job, process) == 0 {
                    if process != 0 {
                        CloseHandle(process as *mut c_void);
                    }
                    CloseHandle(job as *mut c_void);
                    return Group { pid, job: 0 };
                }
                CloseHandle(process as *mut c_void);
                Group { pid, job }
            }
        }

        pub fn interrupt(&self) {
            self.close();
        }

        pub fn kill(&self) {
            self.close();
        }

        fn close(&self) {
            if self.job != 0 {
                // SAFETY: the handle was made by CreateJobObject and is closed once.
                unsafe {
                    TerminateJobObject(self.job, 1);
                }
            } else {
                taskkill(self.pid);
            }
        }
    }

    impl Drop for Group {
        fn drop(&mut self) {
            if self.job != 0 {
                // SAFETY: the last owner closes the job; KILL_ON_JOB_CLOSE takes the tree down.
                unsafe {
                    CloseHandle(self.job as *mut c_void);
                }
            } else {
                taskkill(self.pid);
            }
        }
    }

    /// Fallback for a machine that will not let us own a job; a reused pid is a narrow window.
    fn taskkill(pid: u32) {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}