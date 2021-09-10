use std::ptr;

use winapi::shared::minwindef::BYTE;
use winapi::shared::minwindef::FALSE;
use winapi::shared::minwindef::TRUE;
use winapi::shared::ntdef::WCHAR;
use winapi::shared::winerror::S_OK;
use winapi::um::consoleapi::ClosePseudoConsole;
use winapi::um::consoleapi::CreatePseudoConsole;
use winapi::um::handleapi::CloseHandle;
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::CreateProcessW;
use winapi::um::processthreadsapi::InitializeProcThreadAttributeList;
use winapi::um::processthreadsapi::UpdateProcThreadAttribute;
use winapi::um::processthreadsapi::PROCESS_INFORMATION;
use winapi::um::winbase::CREATE_UNICODE_ENVIRONMENT;
use winapi::um::winbase::EXTENDED_STARTUPINFO_PRESENT;
use winapi::um::winbase::STARTUPINFOEXW;
use winapi::um::wincon::WriteConsoleInputW;
use winapi::um::wincontypes::COORD;
use winapi::um::wincontypes::HPCON;
use winapi::um::wincontypes::INPUT_RECORD;
use winapi::um::wincontypes::KEY_EVENT;
use winapi::um::winnt::HANDLE;

fn main() {
  let stdin = PseudoPipe::new();
  let stdout = PseudoPipe::new();
  let console = PseudoConsole::new(stdin, stdout);

  console.type_char('y');
  console.type_char('y');

  println!("finish")
}

struct PseudoPipe {
  pub read_handle: HANDLE,
  pub write_handle: HANDLE,
}

impl PseudoPipe {
  pub fn new() -> Self {
    unsafe {
      let mut read_handle = std::ptr::null_mut();
      let mut write_handle = std::ptr::null_mut();

      assert_eq!(
        CreatePipe(
          &mut read_handle,
          &mut write_handle,
          std::ptr::null_mut(),
          0
        ),
        TRUE
      );

      Self {
        read_handle,
        write_handle,
      }
    }
  }
}

impl Drop for PseudoPipe {
  fn drop(self: &mut PseudoPipe) {
    unsafe {
      CloseHandle(self.read_handle);
      CloseHandle(self.write_handle);
    }
  }
}

struct PseudoConsole {
  stdin: PseudoPipe,
  stdout: PseudoPipe,
  process_handle: HANDLE,
  thread_handle: HANDLE,
  console_handle: HPCON,
}

impl PseudoConsole {
  pub fn new(stdin: PseudoPipe, stdout: PseudoPipe) -> Self {
    // https://docs.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session
    unsafe {
      let mut size: COORD = std::mem::zeroed();
      size.X = 800;
      size.Y = 500;
      let mut console_handle = std::ptr::null_mut();

      let result = CreatePseudoConsole(
        size,
        stdin.read_handle,
        stdout.write_handle,
        0,
        &mut console_handle,
      );
      assert_eq!(result, S_OK);

      let mut startup_info: STARTUPINFOEXW = std::mem::zeroed();
      startup_info.StartupInfo.cb =
        std::mem::size_of::<STARTUPINFOEXW>() as u32;

      // discover size required for the list
      let mut size = 0;
      assert_eq!(
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size),
        FALSE
      );

      let mut attribute_list = vec![0u8; size];
      startup_info.lpAttributeList = attribute_list.as_mut_ptr() as _;

      assert_eq!(
        InitializeProcThreadAttributeList(
          startup_info.lpAttributeList,
          1,
          0,
          &mut size,
        ),
        TRUE
      );

      let PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE = 0x00020016;
      assert_eq!(
        UpdateProcThreadAttribute(
          startup_info.lpAttributeList,
          0,
          PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
          console_handle,
          std::mem::size_of::<HPCON>(),
          ptr::null_mut(),
          ptr::null_mut(),
        ),
        TRUE
      );

      let mut proc_info: PROCESS_INFORMATION = std::mem::zeroed();
      let mut cmd = to_windows_str("C:\\Windows\\System32\\cmd.exe");
      assert_eq!(
        CreateProcessW(
          ptr::null(),
          cmd.as_mut_ptr(),
          ptr::null_mut(),
          ptr::null_mut(),
          FALSE,
          EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
          ptr::null_mut(),
          ptr::null(),
          &mut startup_info.StartupInfo,
          &mut proc_info,
        ),
        TRUE
      );

      // close the handles that the pseudoconsole now has
      CloseHandle(stdin.read_handle);
      CloseHandle(stdout.write_handle);

      Self {
        stdin,
        stdout,
        console_handle,
        process_handle: proc_info.hProcess,
        thread_handle: proc_info.hThread,
      }
    }
  }

  pub fn type_char(&self, char: char) {
    unsafe {
      let mut input_record: INPUT_RECORD = std::mem::zeroed();
      input_record.EventType = KEY_EVENT;
      input_record.Event.KeyEvent_mut().wRepeatCount = 1;
      *input_record.Event.KeyEvent_mut().uChar.UnicodeChar_mut() =
        char as WCHAR;

      let mut events_written = 0;
      let result = WriteConsoleInputW(
        self.stdin.write_handle,
        &mut input_record,
        1,
        &mut events_written,
      );
      println!(
        "Failing here :( -- Error: {}",
        std::io::Error::last_os_error().to_string()
      );
      assert_eq!(result, TRUE);
      assert_eq!(events_written, 1);
    }
  }
}

impl Drop for PseudoConsole {
  fn drop(self: &mut PseudoConsole) {
    unsafe {
      ClosePseudoConsole(self.console_handle);
      CloseHandle(self.thread_handle);
      CloseHandle(self.process_handle);
    }
  }
}

fn to_windows_str(str: &str) -> Vec<u16> {
  use std::os::windows::prelude::OsStrExt;
  std::ffi::OsStr::new(str)
    .encode_wide()
    .chain(Some(0))
    .collect()
}
