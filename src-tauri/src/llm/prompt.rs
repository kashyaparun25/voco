use crate::commands::meeting::Segment;
use crate::storage::Database;

/// Formats a list of segments into a readable conversational transcript.
pub fn format_transcript(segments: &[Segment]) -> String {
    if segments.is_empty() {
        return "[Empty Transcript]".to_string();
    }
    
    let mut transcript = String::new();
    for seg in segments {
        let speaker = seg.speaker_name.as_deref().unwrap_or("Unknown Speaker");
        transcript.push_str(&format!("{}: {}\n", speaker, seg.text));
    }
    transcript
}

/// Rough token estimate (~4 chars/token) for deciding when a transcript exceeds
/// an LLM's per-request budget and needs map-reduce chunking.
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Split a transcript into chunks each under ~`max_chars`, breaking on line
/// boundaries so speaker turns stay intact. Used by the map step of map-reduce
/// summarization for transcripts too large for one LLM request.
pub fn chunk_transcript(transcript: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for line in transcript.lines() {
        if !cur.is_empty() && cur.len() + line.len() + 1 > max_chars {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Map step: condense ONE chunk of a long transcript into dense notes that
/// preserve every substantive point, so the reduce step can build the final
/// structured summary without having seen the raw transcript.
pub fn chunk_notes_prompt(chunk: &str, index: usize, total: usize) -> String {
    format!(
        "You are processing part {index} of {total} of a long meeting transcript. \
Extract ALL substantive content from THIS part as dense bullet notes: every topic discussed \
(with specifics, numbers, names), every decision or agreement, and every task/commitment with its owner. \
Do not summarize away detail and do not add any preamble or conclusion — output only the notes.\n\n\
Transcript part {index}/{total}:\n\"\"\"\n{chunk}\n\"\"\"\n\nNotes:"
    )
}

/// Retrieves the configured summary length and style settings from the database.
pub fn get_summary_settings(db: &Database) -> (String, String) {
    let length = db
        .get_setting("summary_length")
        .unwrap_or(None)
        .unwrap_or_else(|| "medium".to_string());
    let style = db
        .get_setting("summary_style")
        .unwrap_or(None)
        .unwrap_or_else(|| "default".to_string());
    (length, style)
}

/// Read the configured summary template (Meetily-style presets).
pub fn get_summary_template(db: &Database) -> String {
    db.get_setting("summary_template")
        .unwrap_or(None)
        .unwrap_or_else(|| "general".to_string())
}

/// The section structure a given template should produce. Sections use
/// GitHub-flavored Markdown: `##` headings, `-` bullets, and pipe tables for
/// action items / structured rows (Meetily / Granola / Google-Meet style).
fn template_structure(template: &str) -> &'static str {
    match template {
        "standup" => "\
## Overview\nOne or two sentences on what this standup covered.\n\n\
## Updates by Person\nA `### <Name>` heading for each participant, with bullets: **Done**, **Next**, **Blockers**.\n\n\
## Blockers\n| Blocker | Owner | Needs to unblock |\n| --- | --- | --- |\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "one_on_one" => "\
## Overview\nA short paragraph framing the 1:1.\n\n\
## Discussion Topics\nA `### <Topic>` heading for each topic, with bullets capturing every point raised.\n\n\
## Feedback\nBullets for feedback given and received.\n\n\
## Growth & Goals\nBullets on development, goals, and progress.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |\n\n\
## Follow-ups for Next 1:1\nBulleted list.",
        "sales" => "\
## Overview\nProspect, company, and context in a short paragraph.\n\n\
## Needs & Pain Points\nBulleted list of stated needs/problems.\n\n\
## Objections & Concerns\n| Objection | Response Given |\n| --- | --- |\n\n\
## Proposed Solution\nBullets on the pitch/solution discussed.\n\n\
## Next Steps\n| Owner | Action Item | Due |\n| --- | --- | --- |\n\n\
## Deal Risks\nBulleted list.",
        "interview" => "\
## Overview\nCandidate, role, and interview context.\n\n\
## Background\nBullets on the candidate's relevant experience.\n\n\
## Strengths\nBulleted list with evidence from the conversation.\n\n\
## Concerns / Red Flags\nBulleted list with evidence.\n\n\
## Assessment by Area\n| Area | Assessment |\n| --- | --- |\n\n\
## Recommendation\nHire / No-hire / Follow-up, with rationale.",
        "retrospective" => "\
## Overview\nWhat sprint/period this retro covered.\n\n\
## What Went Well\nBulleted list.\n\n\
## What Didn't Go Well\nBulleted list.\n\n\
## Root Causes\nBulleted list tying problems to causes.\n\n\
## Improvement Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |\n\n\
## Kudos\nBulleted shout-outs.",
        "decision_log" => "\
## Overview\nShort framing of what was decided and why the meeting happened.\n\n\
## Decisions\n| Decision | Rationale | Owner |\n| --- | --- | --- |\n\n\
## Open Questions\nBulleted list of unresolved items.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "customer_discovery" => "\
## Overview\nCustomer, company, and why the call happened, in a short paragraph.\n\n\
## Current Situation\nBullets on their existing process/stack/team and context.\n\n\
## Pain Points\nBulleted list of every stated problem, with severity/impact where mentioned.\n\n\
## Goals & Success Criteria\nBullets on what they want to achieve and how they'd measure it.\n\n\
## Qualification\n| Dimension | Notes |\n| --- | --- |\n(Budget, Authority, Need, Timeline — fill what the conversation supports.)\n\n\
## Next Steps\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "weekly_team_meeting" => "\
## Overview\nA short paragraph on the week's focus and overall status.\n\n\
## Announcements\nBulleted list (omit the section if none).\n\n\
## Updates by Area\nA `### <Area/Person>` heading per stream, with bullets: progress, plans, problems.\n\n\
## Discussion Topics\nA `### <Topic>` heading for each substantive discussion, with the key points raised.\n\n\
## Decisions\nBulleted list with rationale.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "all_hands" => "\
## Overview\nWhat this all-hands covered, in a short paragraph.\n\n\
## Announcements\nBulleted list of company/team announcements.\n\n\
## Metrics & Progress\nBullets on numbers, milestones, and status shared.\n\n\
## Presentations\nA `### <Presenter/Topic>` heading per segment with the key points.\n\n\
## Q&A\n| Question | Answer |\n| --- | --- |\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "board_meeting" => "\
## Overview\nMeeting purpose, attendees, and headline outcome.\n\n\
## Business Update\nBullets on performance, metrics, and highlights presented.\n\n\
## Financials\nBullets on financial results and runway discussed.\n\n\
## Strategic Topics\nA `### <Topic>` heading per topic with the discussion and viewpoints.\n\n\
## Votes & Resolutions\n| Item | Outcome |\n| --- | --- |\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "brainstorm" => "\
## Overview\nThe problem/goal the brainstorm addressed.\n\n\
## Ideas\nA `### <Idea>` heading for each distinct idea, with bullets: description, who raised it, reactions.\n\n\
## Themes\nBullets grouping related ideas.\n\n\
## Shortlisted / Next Ideas to Explore\nBulleted list of what got traction.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "customer_success" => "\
## Overview\nCustomer, account context, and health in a short paragraph.\n\n\
## Current Usage & Adoption\nBullets on how they're using the product.\n\n\
## Issues & Requests\n| Issue/Request | Severity | Status |\n| --- | --- | --- |\n\n\
## Renewal / Expansion Signals\nBullets on risks and opportunities.\n\n\
## Next Steps\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "demo" => "\
## Overview\nWho the demo was for and the outcome.\n\n\
## What Was Shown\nBullets per feature/flow demonstrated.\n\n\
## Reactions & Questions\n| Moment | Reaction/Question |\n| --- | --- |\n\n\
## Objections\nBulleted list with responses given.\n\n\
## Next Steps\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "design_review" => "\
## Overview\nWhat design/work was reviewed and the overall verdict.\n\n\
## Work Presented\nBullets describing what was shown.\n\n\
## Feedback\nA `### <Area>` heading per area with the feedback given (attribute who said it).\n\n\
## Decisions\nBulleted list of what was agreed/changed.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "investor_update" => "\
## Overview\nContext of the conversation and headline takeaways.\n\n\
## Highlights\nBullets on wins and progress shared.\n\n\
## Metrics\n| Metric | Value / Trend |\n| --- | --- |\n\n\
## Challenges & Asks\nBullets on problems raised and help requested.\n\n\
## Investor Feedback\nBulleted list.\n\n\
## Next Steps\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "kickoff" => "\
## Overview\nProject, participants, and goal in a short paragraph.\n\n\
## Goals & Success Criteria\nBulleted list.\n\n\
## Scope\nBullets: in scope / out of scope.\n\n\
## Roles & Responsibilities\n| Person | Role |\n| --- | --- |\n\n\
## Timeline & Milestones\n| Milestone | Date |\n| --- | --- |\n\n\
## Risks & Open Questions\nBulleted list.\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "okr_planning" => "\
## Overview\nPlanning period and framing.\n\n\
## Objectives\nA `### <Objective>` heading per objective, with bullets: key results discussed, owners, targets.\n\n\
## Debates & Tradeoffs\nBullets on disagreements and how they resolved.\n\n\
## Final OKRs\n| Objective | Key Result | Owner |\n| --- | --- | --- |\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "performance_review" => "\
## Overview\nWho, role, and review period.\n\n\
## Achievements\nBulleted list with evidence discussed.\n\n\
## Strengths\nBulleted list.\n\n\
## Growth Areas\nBulleted list with specifics.\n\n\
## Goals for Next Period\n| Goal | Success Measure |\n| --- | --- |\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "user_research" => "\
## Overview\nParticipant profile and research goal.\n\n\
## Key Findings\nBulleted list — the headline insights.\n\n\
## Observations by Topic\nA `### <Topic/Task>` heading per area, with bullets: what the participant did/said (quote verbatim where powerful).\n\n\
## Pain Points\nBulleted list.\n\n\
## Feature Requests & Suggestions\nBulleted list.\n\n\
## Follow-ups\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "support_call" => "\
## Overview\nCustomer, issue, and resolution status in a short paragraph.\n\n\
## Issue Details\nBullets: symptoms, environment, when it started, impact.\n\n\
## Troubleshooting Performed\nBulleted list of steps tried and results.\n\n\
## Resolution / Workaround\nBullets on what fixed it or the current state.\n\n\
## Follow-ups\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "training_session" => "\
## Overview\nWhat the session taught and who attended.\n\n\
## Topics Covered\nA `### <Topic>` heading per topic, with the key points, examples, and demos.\n\n\
## Questions & Answers\n| Question | Answer |\n| --- | --- |\n\n\
## Resources Mentioned\nBulleted list.\n\n\
## Follow-ups\n| Owner | Action Item | Due |\n| --- | --- | --- |",
        "lecture" => "\
## Overview\nSubject, speaker, and scope of the lecture.\n\n\
## Key Concepts\nA `### <Concept>` heading per major concept, with bullets explaining it as presented.\n\n\
## Examples & Case Studies\nBulleted list.\n\n\
## Questions Raised\nBulleted list (from audience or rhetorical).\n\n\
## Further Reading / To Review\nBulleted list.",
        // General / default — comprehensive, everything-covered structure.
        _ => "\
## Overview\nA short paragraph (2-4 sentences) on the meeting's purpose, participants, and outcome.\n\n\
## Attendees\nBulleted list of participants (use the speaker labels from the transcript).\n\n\
## Key Discussion Points\nA `### <Topic>` heading for EACH distinct topic discussed. Under each, bullets capturing every notable point, number, argument, and viewpoint raised — do not omit any topic that got meaningful discussion.\n\n\
## Decisions\nBulleted list of every decision, agreement, or conclusion reached (include the rationale if it was stated).\n\n\
## Action Items\n| Owner | Action Item | Due |\n| --- | --- | --- |\n\n\
## Next Steps & Open Questions\nBulleted list of follow-ups and anything left unresolved.",
    }
}

/// Generates a prompt for summarizing the transcript based on requested length,
/// style, and template. Aims for Google-Meet / Granola quality: comprehensive,
/// well-structured Markdown with tables for action items and structured rows.
/// Resolve a template id to its section structure. Built-ins come from
/// `template_structure`; `custom:<id>` templates are user-authored and read
/// from the `custom_templates` setting (JSON array of
/// `{id, name, emoji, instructions}`) — falling back to the general structure
/// if the id is missing or its instructions are empty.
pub fn resolve_template_structure(db: &Database, template: &str) -> String {
    if let Some(id) = template.strip_prefix("custom:") {
        if let Ok(Some(json)) = db.get_setting("custom_templates") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                for t in v.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                    if t.get("id").and_then(|x| x.as_str()) == Some(id) {
                        if let Some(instr) = t.get("instructions").and_then(|x| x.as_str()) {
                            if !instr.trim().is_empty() {
                                return instr.to_string();
                            }
                        }
                    }
                }
            }
        }
        return template_structure("general").to_string();
    }
    template_structure(template).to_string()
}

pub fn generate_summary_prompt(transcript: &str, length: &str, style: &str, template: &str) -> String {
    generate_summary_prompt_with_structure(transcript, length, style, &template_structure(template).to_string())
}

/// Variant taking an already-resolved section structure (built-in or custom —
/// see `resolve_template_structure`).
pub fn generate_summary_prompt_with_structure(
    transcript: &str,
    length: &str,
    style: &str,
    structure: &str,
) -> String {
    let length_instruction = match length {
        "short" => "LENGTH: Keep it brief — include only the Overview and Action Items sections; a few tight bullets. Skip other sections unless critical.",
        "long" => "LENGTH: Be thorough and comprehensive, like detailed minutes. Cover EVERY topic end-to-end under Key Discussion Points with all relevant specifics — names, numbers, decisions, and differing viewpoints. Do not compress away detail. A longer meeting should yield a longer, multi-section summary that captures everything discussed.",
        _ => "LENGTH: A balanced summary — include all sections, keeping each concise but complete. Don't drop topics that were discussed.",
    };

    let style_instruction = match style {
        "bullet_points" | "bullets" => "Prefer concise bullet points within each section.",
        "detailed" => "Give rich, explanatory detail under each heading.",
        "executive" | "action" => "Write tightly and action-first; lead with what matters and outcomes.",
        _ => "Write clearly and neutrally.",
    };

    format!(
        "You are an expert AI meeting assistant. Read the transcript carefully and write a structured, high-quality meeting summary in GitHub-flavored Markdown.

Produce exactly these sections (omit a section only if there is genuinely nothing for it):

{structure}

Requirements:
- {length_instruction}
- {style_instruction}
- Use real Markdown: `##`/`###` headings, `-` bullets, and pipe tables exactly as shown (with the `| --- |` separator row) for Action Items and any tabular sections.
- Attribute action items and points to the speaker/owner named in the transcript. Use \"—\" when an owner or due date was not stated; never invent one.
- Do NOT invent facts, names, numbers, or decisions. Only use what is in the transcript.
- Do not add a preamble or sign-off — output only the summary itself, starting with the first heading.

Transcript:
\"\"\"
{transcript}
\"\"\"

Write the summary now:",
    )
}
