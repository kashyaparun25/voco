use rusqlite::params;
use anyhow::Result;
use crate::storage::Database;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    pub embedding: Vec<f32>,
    pub created_at: String,
}

pub struct SpeakerProfileManager {
    db: Database,
}

impl SpeakerProfileManager {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn get_all_speakers(&self) -> Result<Vec<SpeakerProfile>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare("SELECT id, name, profile_data, created_at FROM speakers")?;
        
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let profile_data_blob: Option<Vec<u8>> = row.get(2)?;
            let created_at: String = row.get(3)?;
            
            let embedding = profile_data_blob
                .map(|b| blob_to_embedding(&b))
                .unwrap_or_default();
                
            Ok(SpeakerProfile {
                id,
                name,
                embedding,
                created_at,
            })
        })?;

        let mut speakers = Vec::new();
        for r in rows {
            speakers.push(r?);
        }
        Ok(speakers)
    }

    pub fn save_speaker(&self, profile: &SpeakerProfile) -> Result<()> {
        let conn = self.db.conn();
        let blob = embedding_to_blob(&profile.embedding);
        conn.execute(
            "INSERT OR REPLACE INTO speakers (id, name, profile_data, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![profile.id, profile.name, blob, profile.created_at],
        )?;
        Ok(())
    }

    pub fn get_speaker(&self, id: &str) -> Result<Option<SpeakerProfile>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare("SELECT id, name, profile_data, created_at FROM speakers WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let profile_data_blob: Option<Vec<u8>> = row.get(2)?;
            let created_at: String = row.get(3)?;
            
            let embedding = profile_data_blob
                .map(|b| blob_to_embedding(&b))
                .unwrap_or_default();
                
            Ok(Some(SpeakerProfile {
                id,
                name,
                embedding,
                created_at,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_speaker(&self, id: &str) -> Result<()> {
        let conn = self.db.conn();
        conn.execute("DELETE FROM speakers WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_speaker_name(&self, id: &str, name: &str) -> Result<()> {
        let conn = self.db.conn();
        conn.execute("UPDATE speakers SET name = ?2 WHERE id = ?1", params![id, name])?;
        Ok(())
    }
}

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_ne_bytes());
    }
    bytes
}

pub fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    let mut embedding = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        if let Ok(arr) = chunk.try_into() {
            embedding.push(f32::from_ne_bytes(arr));
        }
    }
    embedding
}
