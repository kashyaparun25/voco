use crate::diarization::profiles::{SpeakerProfile, SpeakerProfileManager};
use crate::storage::Database;
use uuid::Uuid;
use log::info;

pub struct SpeakerCluster {
    pub speaker_id: String,
    pub name: String,
    pub centroid: Vec<f32>,
    pub sample_count: usize,
}

pub struct SpeakerClustering {
    profile_manager: SpeakerProfileManager,
    active_clusters: Vec<SpeakerCluster>,
    threshold: f32, // Cosine distance threshold (e.g. 0.20)
}

impl SpeakerClustering {
    pub fn new(db: Database, threshold: f32) -> Self {
        Self {
            profile_manager: SpeakerProfileManager::new(db),
            active_clusters: Vec::new(),
            threshold,
        }
    }

    pub fn match_or_create_speaker(&mut self, embedding: &[f32]) -> (String, String) {
        // 1. Try to match active clusters in the current session
        let mut best_cluster_idx = None;
        let mut min_cluster_dist = f32::MAX;

        for (i, cluster) in self.active_clusters.iter().enumerate() {
            let dist = cosine_distance(embedding, &cluster.centroid);
            if dist < min_cluster_dist {
                min_cluster_dist = dist;
                best_cluster_idx = Some(i);
            }
        }

        if let Some(idx) = best_cluster_idx {
            if min_cluster_dist < self.threshold {
                // Update centroid of the cluster
                let cluster = &mut self.active_clusters[idx];
                cluster.sample_count += 1;
                let n = cluster.sample_count as f32;
                // Running centroid update
                for (c, &e) in cluster.centroid.iter_mut().zip(embedding.iter()) {
                    *c = *c * ((n - 1.0) / n) + e * (1.0 / n);
                }
                // Renormalize centroid
                normalize(&mut cluster.centroid);
                
                info!("Diarization: Matched active cluster {} (name={}) with distance {:.3}", cluster.speaker_id, cluster.name, min_cluster_dist);
                return (cluster.speaker_id.clone(), cluster.name.clone());
            }
        }

        // 2. Try to match global database speakers
        if let Ok(global_speakers) = self.profile_manager.get_all_speakers() {
            let mut best_global = None;
            let mut min_global_dist = f32::MAX;

            for speaker in &global_speakers {
                let dist = cosine_distance(embedding, &speaker.embedding);
                if dist < min_global_dist {
                    min_global_dist = dist;
                    best_global = Some(speaker);
                }
            }

            if let Some(speaker) = best_global {
                if min_global_dist < self.threshold {
                    // Match found! Import to active clusters
                    let cluster = SpeakerCluster {
                        speaker_id: speaker.id.clone(),
                        name: speaker.name.clone(),
                        centroid: speaker.embedding.clone(),
                        sample_count: 1,
                    };
                    self.active_clusters.push(cluster);
                    info!("Diarization: Matched global profile {} (name={}) with distance {:.3}", speaker.id, speaker.name, min_global_dist);
                    return (speaker.id.clone(), speaker.name.clone());
                }
            }
        }

        // 3. Create a new speaker profile
        let new_id = format!("spk_{}", Uuid::new_v4().simple());
        let speaker_num = self.active_clusters.len() + 1;
        let new_name = format!("Speaker {}", speaker_num);
        
        let new_profile = SpeakerProfile {
            id: new_id.clone(),
            name: new_name.clone(),
            embedding: embedding.to_vec(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Save new speaker to global DB
        let _ = self.profile_manager.save_speaker(&new_profile);

        // Add to active clusters
        let cluster = SpeakerCluster {
            speaker_id: new_id.clone(),
            name: new_name.clone(),
            centroid: embedding.to_vec(),
            sample_count: 1,
        };
        self.active_clusters.push(cluster);
        info!("Diarization: Created new speaker cluster {} (name={})", new_id, new_name);

        (new_id, new_name)
    }
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum();
    // Since vectors are normalized, dot is cosine similarity
    (1.0 - dot).max(0.0)
}

fn normalize(vector: &mut [f32]) {
    let sum_sq: f32 = vector.iter().map(|&x| x * x).sum();
    let norm = sum_sq.sqrt();
    if norm > 1e-6 {
        for val in vector.iter_mut() {
            *val /= norm;
        }
    }
}
