use crate::state::AppState;
use chrono::{DateTime, Duration, Local, Timelike};
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Serialize)]
pub struct DayBucket {
    pub label: String,
    pub date: String,
    pub words: u64,
}

#[derive(Serialize)]
pub struct AppCount {
    pub app: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct DictationStats {
    pub wpm: u32,
    pub today_words: u64,
    pub today_saved_seconds: u64,
    pub today_sessions: u64,
    pub total_words: u64,
    pub total_saved_seconds: u64,
    pub current_streak: u64,
    pub best_streak: u64,
    pub transcriptions: u64,
    pub avg_words: u64,
    pub activity7: Vec<DayBucket>,
    pub activity30: Vec<DayBucket>,
    pub active_days7: u64,
    pub words7: u64,
    pub peak_hour: Option<u32>,
    pub top_apps: Vec<AppCount>,
    pub ai_enhanced_pct: u32,
    pub longest_words: u64,
    pub most_words_day: u64,
    pub most_transcriptions_day: u64,
}

/// Aggregate dictation history into the stats shown on the Stats page.
#[tauri::command]
pub fn get_dictation_stats(state: State<'_, AppState>) -> Result<DictationStats, String> {
    let wpm = state
        .db
        .get_setting("typing_wpm")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|w| *w > 0)
        .unwrap_or(40);

    let rows = state.db.dictation_stat_rows().map_err(|e| e.to_string())?;
    let today = Local::now().date_naive();

    let mut per_day_words: HashMap<chrono::NaiveDate, u64> = HashMap::new();
    let mut per_day_count: HashMap<chrono::NaiveDate, u64> = HashMap::new();
    let mut hour_words = [0u64; 24];
    let mut app_counts: HashMap<String, u64> = HashMap::new();
    let mut total_words = 0u64;
    let mut ai_count = 0u64;
    let mut longest_words = 0u64;
    let mut today_words = 0u64;
    let mut today_sessions = 0u64;
    let transcriptions = rows.len() as u64;

    for (created, text, _dur, app, ai) in &rows {
        let words = text.split_whitespace().count() as u64;
        total_words += words;
        longest_words = longest_words.max(words);
        if *ai {
            ai_count += 1;
        }
        if let Some(a) = app {
            let a = a.trim();
            if !a.is_empty() {
                *app_counts.entry(a.to_string()).or_insert(0) += 1;
            }
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(created) {
            let local = dt.with_timezone(&Local);
            let d = local.date_naive();
            *per_day_words.entry(d).or_insert(0) += words;
            *per_day_count.entry(d).or_insert(0) += 1;
            hour_words[local.hour() as usize] += words;
            if d == today {
                today_words += words;
                today_sessions += 1;
            }
        }
    }

    // Current streak: consecutive active days ending today (or yesterday).
    let mut current_streak = 0u64;
    {
        let mut day = today;
        if !per_day_count.contains_key(&day) {
            day = today - Duration::days(1);
        }
        while per_day_count.contains_key(&day) {
            current_streak += 1;
            day -= Duration::days(1);
        }
    }

    // Best streak: longest consecutive run across all active days.
    let mut best_streak = 0u64;
    {
        let mut days: Vec<_> = per_day_count.keys().cloned().collect();
        days.sort();
        let mut run = 0u64;
        let mut prev: Option<chrono::NaiveDate> = None;
        for d in days {
            run = match prev {
                Some(p) if d == p + Duration::days(1) => run + 1,
                _ => 1,
            };
            best_streak = best_streak.max(run);
            prev = Some(d);
        }
    }

    let build = |n: i64| -> Vec<DayBucket> {
        (0..n)
            .rev()
            .map(|i| {
                let d = today - Duration::days(i);
                let words = *per_day_words.get(&d).unwrap_or(&0);
                let label = if n <= 7 {
                    d.format("%a").to_string()
                } else {
                    d.format("%-d").to_string()
                };
                DayBucket { label, date: d.format("%Y-%m-%d").to_string(), words }
            })
            .collect()
    };
    let activity7 = build(7);
    let activity30 = build(30);
    let words7: u64 = activity7.iter().map(|b| b.words).sum();
    let active_days7 = activity7.iter().filter(|b| b.words > 0).count() as u64;

    let peak_hour = if total_words > 0 {
        hour_words
            .iter()
            .enumerate()
            .filter(|(_, w)| **w > 0)
            .max_by_key(|(_, w)| **w)
            .map(|(h, _)| h as u32)
    } else {
        None
    };

    let mut top_apps: Vec<AppCount> = app_counts
        .into_iter()
        .map(|(app, count)| AppCount { app, count })
        .collect();
    top_apps.sort_by(|a, b| b.count.cmp(&a.count));
    top_apps.truncate(3);

    let ai_enhanced_pct = if transcriptions > 0 {
        ((ai_count as f64 / transcriptions as f64) * 100.0).round() as u32
    } else {
        0
    };
    let avg_words = if transcriptions > 0 { total_words / transcriptions } else { 0 };
    let most_words_day = per_day_words.values().cloned().max().unwrap_or(0);
    let most_transcriptions_day = per_day_count.values().cloned().max().unwrap_or(0);

    let saved = |words: u64| -> u64 { (words * 60) / (wpm.max(1) as u64) };

    Ok(DictationStats {
        wpm,
        today_words,
        today_saved_seconds: saved(today_words),
        today_sessions,
        total_words,
        total_saved_seconds: saved(total_words),
        current_streak,
        best_streak,
        transcriptions,
        avg_words,
        activity7,
        activity30,
        active_days7,
        words7,
        peak_hour,
        top_apps,
        ai_enhanced_pct,
        longest_words,
        most_words_day,
        most_transcriptions_day,
    })
}

/// Persist the assumed typing speed used for the "time saved" figure.
#[tauri::command]
pub fn set_typing_wpm(state: State<'_, AppState>, wpm: u32) -> Result<(), String> {
    state
        .db
        .set_setting("typing_wpm", &wpm.max(1).to_string())
        .map_err(|e| e.to_string())
}

/// Clear all dictation history (resets every stat).
#[tauri::command]
pub fn reset_dictation_stats(state: State<'_, AppState>) -> Result<(), String> {
    state.db.clear_dictations().map_err(|e| e.to_string())
}
