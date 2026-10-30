use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Sqlite,
    Mysql,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysqlConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// En memoria siempre trae la clave real; en `config.json` va vacía si está en el llavero del sistema.
    #[serde(default)]
    pub password: String,
    pub database: String,
    /// La clave se guardó en el llavero del sistema (Secret Service / Credential Manager).
    #[serde(default)]
    pub in_keyring: bool,
}

const KEYRING_SERVICE: &str = "pe.mibodega.app";

impl MysqlConfig {
    fn account(&self) -> String {
        format!("mysql:{}@{}:{}/{}", self.user, self.host, self.port, self.database)
    }

    /// Guarda la clave en el llavero del SO. `false` si no hay llavero disponible.
    fn store_password(&self) -> bool {
        keyring::Entry::new(KEYRING_SERVICE, &self.account())
            .and_then(|e| e.set_password(&self.password))
            .is_ok()
    }

    fn load_password(&self) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, &self.account()).ok()?.get_password().ok()
    }
}

impl MysqlConfig {
    pub fn url(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            enc(&self.user),
            enc(&self.password),
            self.host,
            self.port,
            enc(&self.database)
        )
    }
}

/// Percent-encode mínimo para credenciales dentro de la URL.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbConfig {
    #[serde(default)]
    pub engine: Engine,
    #[serde(default)]
    pub mysql: Option<MysqlConfig>,
    /// Carpeta donde se guarda un respaldo automático al cerrar la caja (opcional).
    #[serde(default)]
    pub auto_backup_dir: Option<String>,
}

impl DbConfig {
    /// Carga `config.json` y recupera la clave de MySQL del llavero. Si la clave seguía en texto plano
    /// (versión anterior), la migra al llavero y reescribe el archivo sin ella.
    pub fn load(path: &Path) -> AppResult<Self> {
        let mut cfg: Self = match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        if let Some(m) = cfg.mysql.as_mut() {
            if m.in_keyring {
                m.password = m.load_password().ok_or_else(|| {
                    crate::error::AppError::Validation(
                        "no se pudo leer la clave de MySQL del llavero del sistema; vuelve a ingresarla".into(),
                    )
                })?;
            } else if !m.password.is_empty() {
                let _ = cfg.save(path); // migración oportunista; si no hay llavero, queda como estaba
                if let Some(m) = cfg.mysql.as_mut() {
                    if m.in_keyring {
                        return Ok(Self::load(path)?);
                    }
                }
            }
        }
        Ok(cfg)
    }

    /// Guarda la configuración. La clave de MySQL va al llavero del sistema y NO al archivo;
    /// si no hay llavero disponible se conserva en el archivo (último recurso).
    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut to_write = self.clone();
        if let Some(m) = to_write.mysql.as_mut() {
            if !m.password.is_empty() && m.store_password() {
                m.password.clear();
                m.in_keyring = true;
            } else {
                m.in_keyring = false;
            }
        }
        std::fs::write(path, serde_json::to_string_pretty(&to_write)?)?;
        Ok(())
    }

    pub fn sqlite_path(data_dir: &Path) -> PathBuf {
        data_dir.join("mibodega.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pw: &str) -> MysqlConfig {
        MysqlConfig { host: "127.0.0.1".into(), port: 3306, user: "u".into(), password: pw.into(), database: format!("test-{}", crate::util::new_id()), in_keyring: false }
    }

    /// Con llavero disponible: la clave no queda en config.json y se recupera al cargar.
    /// Sin llavero (CI/headless): se omite.
    #[test]
    fn password_goes_to_keyring_not_to_file() {
        let m = sample("S3creta!");
        if !m.store_password() {
            eprintln!("sin llavero del sistema: se omite");
            return;
        }
        let dir = std::env::temp_dir().join(format!("mibodega-cfg-{}", crate::util::new_id()));
        let path = dir.join("config.json");
        let cfg = DbConfig { engine: Engine::Mysql, mysql: Some(sample("Otra#Clave1")), ..Default::default() };
        cfg.save(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("Otra#Clave1"), "la clave no debe estar en el archivo");
        assert!(raw.contains("\"in_keyring\": true"));
        let loaded = DbConfig::load(&path).unwrap();
        assert_eq!(loaded.mysql.as_ref().unwrap().password, "Otra#Clave1");
        // limpieza del llavero
        for c in [&m, cfg.mysql.as_ref().unwrap()] {
            if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, &c.account()) {
                let _ = e.delete_credential();
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_plaintext_password_is_migrated() {
        let probe = sample("x");
        if !probe.store_password() {
            return;
        }
        let m = sample("Legado#99");
        let dir = std::env::temp_dir().join(format!("mibodega-cfg-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        // formato antiguo: clave en texto plano, sin in_keyring
        std::fs::write(&path, serde_json::json!({"engine":"mysql","mysql":{"host":m.host,"port":m.port,"user":m.user,"password":m.password,"database":m.database}}).to_string()).unwrap();
        let loaded = DbConfig::load(&path).unwrap();
        assert_eq!(loaded.mysql.as_ref().unwrap().password, "Legado#99");
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("Legado#99"), "tras cargar, la clave ya no está en el archivo");
        for c in [&probe, &m] {
            if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, &c.account()) {
                let _ = e.delete_credential();
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
