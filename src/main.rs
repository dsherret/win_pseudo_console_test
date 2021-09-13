use std::io::BufRead;
use std::io::BufReader;
use std::ptr;
use std::io::Write;

use winapi::shared::minwindef::FALSE;
use winapi::shared::minwindef::TRUE;
use winapi::shared::winerror::S_OK;
use winapi::um::consoleapi::ClosePseudoConsole;
use winapi::um::consoleapi::CreatePseudoConsole;
use winapi::um::fileapi::ReadFile;
use winapi::um::fileapi::WriteFile;
use winapi::um::handleapi::CloseHandle;
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::CreateProcessW;
use winapi::um::processthreadsapi::DeleteProcThreadAttributeList;
use winapi::um::processthreadsapi::InitializeProcThreadAttributeList;
use winapi::um::processthreadsapi::UpdateProcThreadAttribute;
use winapi::um::processthreadsapi::LPPROC_THREAD_ATTRIBUTE_LIST;
use winapi::um::processthreadsapi::PROCESS_INFORMATION;
use winapi::um::winbase::CREATE_UNICODE_ENVIRONMENT;
use winapi::um::winbase::EXTENDED_STARTUPINFO_PRESENT;
use winapi::um::winbase::STARTUPINFOEXW;
use winapi::um::wincontypes::COORD;
use winapi::um::wincontypes::HPCON;
use winapi::um::winnt::HANDLE;

fn main() {
  let stdin = PseudoPipe::new();
  let stdout = PseudoPipe::new();
  let mut console = PseudoConsole::new(stdin, stdout);

  let console_reader = BufReader::new(console.get_reader());
  let mut lines = console_reader.lines();
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());

  let text = "echo Hello\r\n\x1b[A\r\n";
  console.write(text.as_bytes()).unwrap();

  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());
  println!("{}", lines.next().unwrap().unwrap());

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
          ptr::null_mut(),
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

      let mut attribute_list = ProcThreadAttributeList::new(console_handle);
      let mut startup_info: STARTUPINFOEXW = std::mem::zeroed();
      startup_info.StartupInfo.cb =
        std::mem::size_of::<STARTUPINFOEXW>() as u32;
      startup_info.lpAttributeList = attribute_list.as_mut_ptr();

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

  pub fn get_reader(&self) -> ConsoleReader {
    ConsoleReader { read_handle: self.stdout.read_handle }
  }
}

impl std::io::Write for PseudoConsole {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
      unsafe {
        let mut bytes_written = 0;
        assert_eq!(
          WriteFile(
            self.stdin.write_handle,
            buffer.as_ptr() as *const _,
            buffer.len() as u32,
            &mut bytes_written,
            ptr::null_mut(),
          ),
          TRUE
        );

        Ok(bytes_written as usize)
      }
    }

    fn flush(&mut self) -> std::io::Result<()> {
      Ok(())
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

struct ConsoleReader {
  read_handle: HANDLE,
}

impl std::io::Read for ConsoleReader {
  fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
    unsafe {
      let mut bytes_read = 0;
      assert_eq!(
        ReadFile(
          self.read_handle,
          buffer.as_mut_ptr() as _,
          buffer.len() as u32,
          &mut bytes_read,
          ptr::null_mut(),
        ),
        TRUE
      );
      Ok(bytes_read as usize)
    }
  }
}

struct ProcThreadAttributeList {
  buffer: Vec<u8>,
}

impl ProcThreadAttributeList {
  pub fn new(console_handle: HPCON) -> Self {
    unsafe {
      // discover size required for the list
      let mut size = 0;
      let attribute_count = 1;
      assert_eq!(
        InitializeProcThreadAttributeList(
          ptr::null_mut(),
          attribute_count,
          0,
          &mut size
        ),
        FALSE
      );

      let mut buffer = vec![0u8; size];
      let attribute_list_ptr = buffer.as_mut_ptr() as _;

      assert_eq!(
        InitializeProcThreadAttributeList(
          attribute_list_ptr,
          attribute_count,
          0,
          &mut size,
        ),
        TRUE
      );

      const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;
      assert_eq!(
        UpdateProcThreadAttribute(
          attribute_list_ptr,
          0,
          PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
          console_handle,
          std::mem::size_of::<HPCON>(),
          ptr::null_mut(),
          ptr::null_mut(),
        ),
        TRUE
      );

      ProcThreadAttributeList { buffer }
    }
  }

  pub fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
    self.buffer.as_mut_slice().as_mut_ptr() as *mut _
  }
}

impl Drop for ProcThreadAttributeList {
  fn drop(&mut self) {
    unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
  }
}

fn to_windows_str(str: &str) -> Vec<u16> {
  use std::os::windows::prelude::OsStrExt;
  std::ffi::OsStr::new(str)
    .encode_wide()
    .chain(Some(0))
    .collect()
}
