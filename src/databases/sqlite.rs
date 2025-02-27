use std::str::FromStr;

use async_trait::async_trait;
use log::debug;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

use crate::databases::{Database, Error};
use crate::protocol::clock::DurationSinceUnixEpoch;
use crate::protocol::info_hash::InfoHash;
use crate::tracker::auth;

pub struct Sqlite {
    pool: Pool<SqliteConnectionManager>,
}

impl Sqlite {
    /// # Errors
    ///
    /// Will return `r2d2::Error` if `db_path` is not able to create `SqLite` database.
    pub fn new(db_path: &str) -> Result<Sqlite, r2d2::Error> {
        let cm = SqliteConnectionManager::file(db_path);
        let pool = Pool::new(cm).expect("Failed to create r2d2 SQLite connection pool.");
        Ok(Sqlite { pool })
    }
}

#[async_trait]
impl Database for Sqlite {
    fn create_database_tables(&self) -> Result<(), Error> {
        let create_whitelist_table = "
        CREATE TABLE IF NOT EXISTS whitelist (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            info_hash TEXT NOT NULL UNIQUE
        );"
        .to_string();

        let create_torrents_table = "
        CREATE TABLE IF NOT EXISTS torrents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            info_hash TEXT NOT NULL UNIQUE,
            completed INTEGER DEFAULT 0 NOT NULL
        );"
        .to_string();

        let create_keys_table = "
        CREATE TABLE IF NOT EXISTS keys (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key TEXT NOT NULL UNIQUE,
            valid_until INTEGER NOT NULL
         );"
        .to_string();

        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        conn.execute(&create_whitelist_table, [])
            .and_then(|_| conn.execute(&create_keys_table, []))
            .and_then(|_| conn.execute(&create_torrents_table, []))
            .map_err(|_| Error::InvalidQuery)
            .map(|_| ())
    }

    async fn load_persistent_torrents(&self) -> Result<Vec<(InfoHash, u32)>, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        let mut stmt = conn.prepare("SELECT info_hash, completed FROM torrents")?;

        let torrent_iter = stmt.query_map([], |row| {
            let info_hash_string: String = row.get(0)?;
            let info_hash = InfoHash::from_str(&info_hash_string).unwrap();
            let completed: u32 = row.get(1)?;
            Ok((info_hash, completed))
        })?;

        let torrents: Vec<(InfoHash, u32)> = torrent_iter.filter_map(std::result::Result::ok).collect();

        Ok(torrents)
    }

    async fn load_keys(&self) -> Result<Vec<auth::Key>, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        let mut stmt = conn.prepare("SELECT key, valid_until FROM keys")?;

        let keys_iter = stmt.query_map([], |row| {
            let key = row.get(0)?;
            let valid_until: i64 = row.get(1)?;

            Ok(auth::Key {
                key,
                valid_until: Some(DurationSinceUnixEpoch::from_secs(valid_until.unsigned_abs())),
            })
        })?;

        let keys: Vec<auth::Key> = keys_iter.filter_map(std::result::Result::ok).collect();

        Ok(keys)
    }

    async fn load_whitelist(&self) -> Result<Vec<InfoHash>, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        let mut stmt = conn.prepare("SELECT info_hash FROM whitelist")?;

        let info_hash_iter = stmt.query_map([], |row| {
            let info_hash: String = row.get(0)?;

            Ok(InfoHash::from_str(&info_hash).unwrap())
        })?;

        let info_hashes: Vec<InfoHash> = info_hash_iter.filter_map(std::result::Result::ok).collect();

        Ok(info_hashes)
    }

    async fn save_persistent_torrent(&self, info_hash: &InfoHash, completed: u32) -> Result<(), Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        match conn.execute(
            "INSERT INTO torrents (info_hash, completed) VALUES (?1, ?2) ON CONFLICT(info_hash) DO UPDATE SET completed = ?2",
            [info_hash.to_string(), completed.to_string()],
        ) {
            Ok(updated) => {
                if updated > 0 {
                    return Ok(());
                }
                Err(Error::QueryReturnedNoRows)
            }
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }

    async fn get_info_hash_from_whitelist(&self, info_hash: &str) -> Result<InfoHash, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        let mut stmt = conn.prepare("SELECT info_hash FROM whitelist WHERE info_hash = ?")?;
        let mut rows = stmt.query([info_hash])?;

        match rows.next() {
            Ok(row) => match row {
                Some(row) => Ok(InfoHash::from_str(&row.get_unwrap::<_, String>(0)).unwrap()),
                None => Err(Error::QueryReturnedNoRows),
            },
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }

    async fn add_info_hash_to_whitelist(&self, info_hash: InfoHash) -> Result<usize, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        match conn.execute("INSERT INTO whitelist (info_hash) VALUES (?)", [info_hash.to_string()]) {
            Ok(updated) => {
                if updated > 0 {
                    return Ok(updated);
                }
                Err(Error::QueryReturnedNoRows)
            }
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }

    async fn remove_info_hash_from_whitelist(&self, info_hash: InfoHash) -> Result<usize, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        match conn.execute("DELETE FROM whitelist WHERE info_hash = ?", [info_hash.to_string()]) {
            Ok(updated) => {
                if updated > 0 {
                    return Ok(updated);
                }
                Err(Error::QueryReturnedNoRows)
            }
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }

    async fn get_key_from_keys(&self, key: &str) -> Result<auth::Key, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        let mut stmt = conn.prepare("SELECT key, valid_until FROM keys WHERE key = ?")?;
        let mut rows = stmt.query([key.to_string()])?;

        if let Some(row) = rows.next()? {
            let key: String = row.get(0).unwrap();
            let valid_until: i64 = row.get(1).unwrap();

            Ok(auth::Key {
                key,
                valid_until: Some(DurationSinceUnixEpoch::from_secs(valid_until.unsigned_abs())),
            })
        } else {
            Err(Error::QueryReturnedNoRows)
        }
    }

    async fn add_key_to_keys(&self, auth_key: &auth::Key) -> Result<usize, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        match conn.execute(
            "INSERT INTO keys (key, valid_until) VALUES (?1, ?2)",
            [auth_key.key.to_string(), auth_key.valid_until.unwrap().as_secs().to_string()],
        ) {
            Ok(updated) => {
                if updated > 0 {
                    return Ok(updated);
                }
                Err(Error::QueryReturnedNoRows)
            }
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }

    async fn remove_key_from_keys(&self, key: &str) -> Result<usize, Error> {
        let conn = self.pool.get().map_err(|_| Error::DatabaseError)?;

        match conn.execute("DELETE FROM keys WHERE key = ?", [key]) {
            Ok(updated) => {
                if updated > 0 {
                    return Ok(updated);
                }
                Err(Error::QueryReturnedNoRows)
            }
            Err(e) => {
                debug!("{:?}", e);
                Err(Error::InvalidQuery)
            }
        }
    }
}
