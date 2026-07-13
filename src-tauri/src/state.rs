use crate::storage::Database;
use crate::stt::ModelManager;
use crate::services::{DictationService, MeetingService};
use crate::audio::MicCapture;
use parking_lot::Mutex;

pub struct AppState {
    pub db: Database,
    pub model_manager: ModelManager,
    pub dictation_service: DictationService,
    pub meeting_service: MeetingService,
    pub active_capture: Mutex<Option<MicCapture>>,
}

impl AppState {
    pub fn new(db: Database, model_manager: ModelManager) -> Self {
        let dictation_service = DictationService::new(db.clone(), model_manager.clone());
        let meeting_service = MeetingService::new(db.clone(), model_manager.clone());
        Self {
            db,
            model_manager,
            dictation_service,
            meeting_service,
            active_capture: Mutex::new(None),
        }
    }
}
