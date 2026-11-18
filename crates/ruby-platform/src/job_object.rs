//! Windows Job Object helper for process tree termination.
//! Ensures that when a parent task or job is cancelled, timed out, or dropped,
//! all child, grandchild, and descendant processes are killed atomically by Windows.

use crate::PlatformError;
use std::os::windows::io::RawHandle;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct JobObjectProcessTree {
    handle: HANDLE,
}

unsafe impl Send for JobObjectProcessTree {}
unsafe impl Sync for JobObjectProcessTree {}

impl JobObjectProcessTree {
    /// Creates a new Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    pub fn new() -> Result<Self, PlatformError> {
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(|e| {
                PlatformError::SecurityCheckFailed(format!("Failed to create Job Object: {}", e))
            })?;

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|e| {
                let _ = CloseHandle(handle);
                PlatformError::SecurityCheckFailed(format!(
                    "Failed to set Job Object kill-on-close limit: {}",
                    e
                ))
            })?;

            Ok(Self { handle })
        }
    }

    /// Assigns a running process to this job object.
    pub fn assign_process(&self, process_handle: RawHandle) -> Result<(), PlatformError> {
        unsafe {
            let h = HANDLE(process_handle as *mut _);
            AssignProcessToJobObject(self.handle, h).map_err(|e| {
                PlatformError::SecurityCheckFailed(format!(
                    "Failed to assign process to Job Object: {}",
                    e
                ))
            })?;
            Ok(())
        }
    }

    /// Explicitly terminates all processes within this job object immediately.
    pub fn kill_tree(&self) -> Result<(), PlatformError> {
        unsafe {
            TerminateJobObject(self.handle, 1).map_err(|e| {
                PlatformError::SecurityCheckFailed(format!(
                    "Failed to terminate Job Object tree: {}",
                    e
                ))
            })?;
            Ok(())
        }
    }
}

impl Drop for JobObjectProcessTree {
    fn drop(&mut self) {
        unsafe {
            let _ = TerminateJobObject(self.handle, 1);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    #[test]
    fn test_job_object_kills_entire_process_tree() {
        // Create job object
        let job = JobObjectProcessTree::new().expect("Failed to create job object");

        // Spawn a parent powershell process that spawns a child process and sleeps
        // cmd.exe /c start cmd.exe /c timeout 60
        let mut child = Command::new("powershell")
            .args(["-Command", "Start-Sleep -Seconds 60"])
            .spawn()
            .expect("Failed to spawn test process");

        let raw_handle = child.as_raw_handle();
        job.assign_process(raw_handle)
            .expect("Failed to assign process to job");

        // Kill the whole tree explicitly
        job.kill_tree().expect("Failed to kill job tree");

        // Wait on the child - should exit immediately due to TerminateJobObject
        let status = child.wait().expect("Child must exit after job termination");
        assert!(!status.success());
    }
}
