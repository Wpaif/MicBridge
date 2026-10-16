use super::Take;
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub struct ExportError {
    pub completed: Vec<PathBuf>,
    pub source: io::Error,
}
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; arquivos concluídos: {}",
            self.source,
            self.completed.len()
        )
    }
}
impl std::error::Error for ExportError {}
pub fn encode(samples: &[f32]) -> io::Result<Vec<u8>> {
    let len = samples
        .len()
        .checked_mul(2)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= u32::MAX - 36)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV grande demais"))?;
    let mut bytes = Vec::with_capacity(44 + len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48000u32.to_le_bytes());
    bytes.extend_from_slice(&96000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&len.to_le_bytes());
    for &sample in samples {
        let sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let pcm = (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    Ok(bytes)
}
fn same_destination(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if a == b {
        return true;
    }
    if let (Ok(a), Ok(b)) = (std::fs::metadata(a), std::fs::metadata(b)) {
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return true;
        }
    }
    let normalized = |p: &Path| {
        p.parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .ok()
            .and_then(|parent| p.file_name().map(|name| parent.join(name)))
    };
    match (normalized(a), normalized(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
/// Export without permission to replace any existing destination.
pub fn export(take: &Take, original: &Path, treated: &Path) -> Result<(), ExportError> {
    export_with_overwrite(take, original, treated, false)
}
/// Only pass `overwrite = true` after explicit confirmation for both paths.
pub fn export_with_overwrite(
    take: &Take,
    original: &Path,
    treated: &Path,
    overwrite: bool,
) -> Result<(), ExportError> {
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let mut completed = Vec::new();
    if take.original.is_empty()
        || take.original.len() != take.treated.len()
        || same_destination(original, treated)
    {
        return Err(ExportError {
            completed,
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "Teste vazio, amostras desalinhadas ou destinos iguais",
            ),
        });
    }
    // Reject dangling and non-dangling symbolic links before creating either WAV.
    // O_NOFOLLOW below also closes the race between this check and open().
    for path in [original, treated] {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if !metadata.is_file() {
                return Err(ExportError {
                    completed,
                    source: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Escolha arquivos regulares, sem links simbólicos, para os dois WAVs",
                    ),
                });
            }
        }
    }
    let mut opened: Vec<File> = Vec::new();
    for (path, samples) in [(original, &take.original), (treated, &take.treated)] {
        let result = (|| -> io::Result<File> {
            let bytes = encode(samples)?;
            let mut options = OpenOptions::new();
            options
                .write(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            if overwrite {
                options.create(true);
            } else {
                options.create_new(true);
            }
            let mut file = options.open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || opened.iter().any(|other| {
                    other
                        .metadata()
                        .is_ok_and(|m| m.dev() == metadata.dev() && m.ino() == metadata.ino())
                })
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Os destinos precisam ser arquivos regulares distintos",
                ));
            }
            // Opening must not truncate until the actual file identity is checked.
            file.set_len(0)?;
            file.write_all(&bytes)?;
            file.flush()?;
            Ok(file)
        })();
        match result {
            Ok(file) => {
                opened.push(file);
                completed.push(path.to_owned());
            }
            Err(source) => return Err(ExportError { completed, source }),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "micbridge-wav-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn take() -> Take {
        Take {
            id: 1,
            original: vec![-1.0, 0.5],
            treated: vec![0.0, 1.0],
        }
    }
    #[test]
    fn wav_header_and_pcm_are_readable_independently() {
        let b = encode(&[
            -2.0,
            -1.0,
            0.0,
            1.0,
            2.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ])
        .unwrap();
        assert_eq!(b.len(), 60);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(&b[36..40], b"data");
        let u16at = |i| u16::from_le_bytes(b[i..i + 2].try_into().unwrap());
        let u32at = |i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(u32at(4), 52);
        assert_eq!(u32at(16), 16);
        assert_eq!(u16at(20), 1);
        assert_eq!(u16at(22), 1);
        assert_eq!(u32at(24), 48000);
        assert_eq!(u32at(28), 96000);
        assert_eq!(u16at(32), 2);
        assert_eq!(u16at(34), 16);
        assert_eq!(u32at(40), 16);
        let pcm: Vec<i16> = b[44..]
            .chunks_exact(2)
            .map(|x| i16::from_le_bytes(x.try_into().unwrap()))
            .collect();
        assert_eq!(pcm, [-32768, -32768, 0, 32767, 32767, 0, 0, 0]);
    }
    #[test]
    fn exports_both_versions_with_distinct_samples() {
        let d = Temp::new();
        let a = d.0.join("a.wav");
        let b = d.0.join("b.wav");
        export(&take(), &a, &b).unwrap();
        let a = std::fs::read(a).unwrap();
        let b = std::fs::read(b).unwrap();
        assert_eq!(a.len(), 48);
        assert_eq!(b.len(), 48);
        assert_eq!(&a[44..], &[0, 128, 0, 64]);
        assert_eq!(&b[44..], &[0, 0, 255, 127]);
    }
    #[test]
    fn rejects_same_destination_before_any_write() {
        let d = Temp::new();
        let p = d.0.join("same.wav");
        assert!(export(&take(), &p, &p).is_err());
        assert!(!p.exists());
        std::fs::write(&p, b"preservar").unwrap();
        let link = d.0.join("alias.wav");
        std::fs::hard_link(&p, &link).unwrap();
        assert!(export(&take(), &p, &link).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"preservar");
    }
    #[test]
    fn reports_partial_export_and_failure_of_first_destination() {
        let d = Temp::new();
        let a = d.0.join("a.wav");
        let bad = d.0.join("missing/b.wav");
        let e = export(&take(), &bad, &a).unwrap_err();
        assert!(e.completed.is_empty());
        assert!(!a.exists());
        let e = export(&take(), &a, &bad).unwrap_err();
        assert_eq!(e.completed, [a.clone()]);
        assert!(a.exists());
    }
    #[test]
    fn rejects_dangling_symlink_alias_before_creating_original() {
        let d = Temp::new();
        let a = d.0.join("a.wav");
        let b = d.0.join("b.wav");
        std::os::unix::fs::symlink(&a, &b).unwrap();
        assert!(export(&take(), &a, &b).is_err());
        assert!(!a.exists());
    }
    #[test]
    fn unconfirmed_export_never_overwrites_an_existing_destination() {
        let d = Temp::new();
        let a = d.0.join("a.wav");
        let b = d.0.join("b.wav");
        // This also covers a destination created after the chooser's initial check:
        // the writer must protect it without depending on that check.
        assert!(!b.exists());
        std::fs::write(&b, b"must survive").unwrap();
        let error = export(&take(), &a, &b).unwrap_err();
        assert_eq!(std::fs::read(&b).unwrap(), b"must survive");
        assert_eq!(error.completed, [a]);
    }
    #[test]
    fn rejects_empty_or_unpaired_take() {
        let d = Temp::new();
        let a = d.0.join("a.wav");
        let b = d.0.join("b.wav");
        for t in [
            Take {
                id: 1,
                original: vec![],
                treated: vec![],
            },
            Take {
                id: 2,
                original: vec![0.1],
                treated: vec![],
            },
        ] {
            assert!(export(&t, &a, &b).is_err());
            assert!(!a.exists());
            assert!(!b.exists());
        }
    }
}
