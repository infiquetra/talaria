use std::{fs::File, io, path::Path};

/// Open attacker-controlled input without following a final symlink or waiting on a named pipe.
pub(crate) fn open_read_only(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        )?;
        Ok(File::from(fd))
    }
    #[cfg(not(unix))]
    {
        File::open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn named_pipe_open_does_not_wait_for_a_writer() {
        let temp = tempfile::tempdir().unwrap();
        let pipe = temp.path().join("input.pipe");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .unwrap()
                .success()
        );

        let file = open_read_only(&pipe).unwrap();
        assert!(!file.metadata().unwrap().is_file());
    }
}
