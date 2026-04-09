use std::io;
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub const AF_VSOCK: i32 = 40;
pub const VMADDR_CID_ANY: u32 = u32::MAX;
pub const VMADDR_CID_HOST: u32 = 2;

#[repr(C)]
#[derive(Copy, Clone)]
struct SockAddrVm {
    svm_family: libc::sa_family_t,
    svm_reserved1: libc::c_ushort,
    svm_port: u32,
    svm_cid: u32,
    svm_zero: [u8; 4],
}

pub struct VsockListener {
    fd: OwnedFd,
}

impl VsockListener {
    pub fn bind(port: u32) -> io::Result<Self> {
        let fd = unsafe { libc::socket(AF_VSOCK, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let addr = SockAddrVm {
            svm_family: AF_VSOCK as libc::sa_family_t,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: VMADDR_CID_ANY,
            svm_zero: [0; 4],
        };

        let rc = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &addr as *const SockAddrVm as *const libc::sockaddr,
                size_of::<SockAddrVm>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }

        let rc = unsafe { libc::listen(fd.as_raw_fd(), 16) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { fd })
    }

    pub fn accept(&self) -> io::Result<VsockStream> {
        let mut addr = SockAddrVm {
            svm_family: 0,
            svm_reserved1: 0,
            svm_port: 0,
            svm_cid: 0,
            svm_zero: [0; 4],
        };
        let mut len = size_of::<SockAddrVm>() as libc::socklen_t;

        let client_fd = unsafe {
            libc::accept(
                self.fd.as_raw_fd(),
                &mut addr as *mut SockAddrVm as *mut libc::sockaddr,
                &mut len,
            )
        };
        if client_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let fd = unsafe { OwnedFd::from_raw_fd(client_fd) };
        Ok(VsockStream {
            fd,
            peer_cid: addr.svm_cid,
            peer_port: addr.svm_port,
        })
    }
}

pub struct VsockStream {
    fd: OwnedFd,
    pub peer_cid: u32,
    pub peer_port: u32,
}

impl VsockStream {
    pub fn connect(cid: u32, port: u32) -> io::Result<Self> {
        let fd = unsafe { libc::socket(AF_VSOCK, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let addr = SockAddrVm {
            svm_family: AF_VSOCK as libc::sa_family_t,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: cid,
            svm_zero: [0; 4],
        };

        let rc = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                &addr as *const SockAddrVm as *const libc::sockaddr,
                size_of::<SockAddrVm>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            fd,
            peer_cid: cid,
            peer_port: port,
        })
    }
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        let tv = match timeout {
            Some(dur) => libc::timeval {
                tv_sec: dur.as_secs() as libc::time_t,
                tv_usec: dur.subsec_micros() as libc::suseconds_t,
            },
            None => libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
        };

        let rc = unsafe {
            libc::setsockopt(
                self.fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };

        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl std::io::Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let rc = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }
}

impl std::io::Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let rc = unsafe {
            libc::write(
                self.fd.as_raw_fd(),
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
            )
        };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
