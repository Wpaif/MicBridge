use super::graph::SavedLink;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};
const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    entries: Vec<SavedLink>,
}
pub struct Journal {
    path: PathBuf,
    _lock: File,
}
impl Journal {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let parent = path.parent().ok_or("Registro sem diretório")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        // SAFETY: flock receives a live file descriptor and valid nonblocking flags.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(format!(
                "Outra instância controla as conexões de áudio: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { path, _lock: lock })
    }
    pub fn load(&self) -> Result<Vec<SavedLink>, String> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.to_string()),
        };
        let mut bytes = vec![];
        file.take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > LIMIT {
            return Err("Registro excedeu 8 MiB".into());
        }
        let r: Record = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Registro de conexões inválido: {e}"))?;
        if r.version != 1 || r.entries.len() > 4096 {
            return Err("Versão ou tamanho do registro inválido".into());
        }
        Ok(r.entries)
    }
    pub fn save(&self, entries: &[SavedLink]) -> Result<(), String> {
        let mut unique: Vec<SavedLink> = vec![];
        for e in entries {
            if !unique
                .iter()
                .any(|u| u.core == e.core && u.output == e.output && u.input == e.input)
            {
                unique.push(e.clone());
            }
        }
        if unique.len() > 4096 {
            return Err("Limite de 4096 conexões excedido".into());
        }
        let bytes = serde_json::to_vec(&Record {
            version: 1,
            entries: unique,
        })
        .map_err(|e| e.to_string())?;
        if bytes.len() > LIMIT {
            return Err("Registro excedeu 8 MiB".into());
        }
        let temp = self.path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            crate::state::random_hex(8)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            std::fs::rename(&temp, &self.path)?;
            File::open(self.path.parent().unwrap())?.sync_all()
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(|e| format!("Não foi possível registrar conexões: {e}"))
    }
    pub fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => File::open(self.path.parent().unwrap())
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio_test::Fixture,
        monitor::routing::graph::tests::{graph, target},
    };
    #[test]
    fn journal_survives_interruption_before_or_after_unlink() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("journal.json");
        let entries = graph()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap();
        {
            let j = Journal::open(path.clone()).unwrap();
            j.save(&entries).unwrap();
        }
        let j = Journal::open(path.clone()).unwrap();
        assert_eq!(j.load().unwrap(), entries);
        j.clear().unwrap();
        assert!(j.load().unwrap().is_empty());
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(j.load().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
        std::fs::write(&path, vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
        assert!(j.load().is_err());
    }
    #[test]
    fn journal_save_failure_prevents_unlink_and_second_owner_is_rejected() {
        let f = Fixture::new("exit 0");
        let path = f.dir.join("journal.json");
        let j = Journal::open(path.clone()).unwrap();
        assert!(
            Journal::open(path.clone()).is_err(),
            "second owner must not acquire lock"
        );
        let entries = graph()
            .competitors(&target(), "micbridge_monitor_test")
            .unwrap();
        j.save(&[entries[0].clone(), entries[0].clone()]).unwrap();
        assert_eq!(j.load().unwrap().len(), 1);
        std::fs::create_dir(f.dir.join("blocked")).unwrap();
        let broken = Journal::open(f.dir.join("blocked")).unwrap();
        assert!(broken.save(&entries).is_err());
        let mut huge = vec![];
        for id in 0..4097 {
            let mut e = entries[0].clone();
            e.output.port.serial = id;
            huge.push(e);
        }
        assert!(j.save(&huge).is_err());
        drop(j);
        assert!(Journal::open(path).is_ok());
    }
}
