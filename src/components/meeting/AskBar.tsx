import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { parseMarkdown } from "./SummaryView";

/** Quick-prompt "recipes" shown above the Ask-anything input. Add more here. */
export const RECIPES: Array<{ label: string; prompt: string }> = [
  { label: "List action items", prompt: "List every action item discussed, with owners if mentioned." },
  { label: "Key decisions", prompt: "What decisions were made? Bullet each with a one-line rationale." },
  { label: "Draft follow-up email", prompt: "Draft a short, professional follow-up email summarizing outcomes and next steps." },
];

interface AskBarProps {
  /** Meeting to ask about, or null to answer from recent meeting summaries. */
  meetingId: string | null;
  /** Optional slot rendered at the left of the input row (e.g. transcript toggle). */
  leading?: ReactNode;
  /** While true the input + recipes are replaced by `liveControls`. */
  live?: boolean;
  /** Live-meeting controls (elapsed / pause / stop) shown while recording. */
  liveControls?: ReactNode;
  /** Extra chips rendered before the recipe chips (e.g. Generate notes). */
  extraChips?: ReactNode;
}

const ArrowUpIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2.2} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 19.5v-15m0 0-6.75 6.75M12 4.5l6.75 6.75" />
  </svg>
);

const CloseIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
  </svg>
);

const Spinner = ({ size = 12 }: { size?: number }) => (
  <span
    style={{
      width: size,
      height: size,
      borderRadius: "50%",
      border: "2px solid currentColor",
      borderTopColor: "transparent",
      display: "inline-block",
      animation: "voco-spin 0.7s linear infinite",
      flexShrink: 0,
    }}
  />
);

/**
 * AskBar — Granola-style "Ask anything" floating bar, docked bottom-center.
 *
 * Submits questions to the backend `ask_meeting_ai` command and shows the
 * answer in a dismissible panel that grows above the bar. On the meeting
 * note page it also hosts the transcript toggle (via `leading`) and the
 * live-recording controls (via `live` / `liveControls`).
 */
export default function AskBar({ meetingId, leading, live, liveControls, extraChips }: AskBarProps) {
  const [question, setQuestion] = useState("");
  const [pending, setPending] = useState(false);
  const [lastQ, setLastQ] = useState("");
  const [answer, setAnswer] = useState<string | null>(null);
  const [isError, setIsError] = useState(false);

  const submit = async (raw: string) => {
    const q = raw.trim();
    if (!q || pending) return;
    setPending(true);
    setLastQ(q);
    setQuestion("");
    setAnswer(null);
    try {
      const res = await invoke<string>("ask_meeting_ai", {
        meetingId,
        question: q,
        requestId: crypto.randomUUID(),
      });
      setAnswer(res);
      setIsError(false);
    } catch (err) {
      // Errors are user-readable strings (e.g. no notes yet) — show inline.
      setAnswer(typeof err === "string" ? err : "Something went wrong — please try again.");
      setIsError(true);
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="mtg-askwrap">
      {/* Answer panel (grows above the bar) */}
      {answer !== null && (
        <div className="mtg-ask-answer" role="region" aria-label="AI answer">
          <div className="mtg-ask-answer-head">
            <span className="mtg-ask-q" title={lastQ}>Q: {lastQ}</span>
            <button className="mtg-iconbtn" onClick={() => setAnswer(null)} title="Dismiss" aria-label="Dismiss answer">
              <CloseIcon />
            </button>
          </div>
          <div className="mtg-ask-answer-body">
            {isError ? (
              <p className="mtg-ask-error">{answer}</p>
            ) : (
              parseMarkdown(answer)
            )}
          </div>
        </div>
      )}

      <div className="mtg-askbar">
        {/* Recipe chips row (hidden while a meeting is recording) */}
        {!live && (
          <div className="mtg-ask-chips">
            {extraChips}
            {RECIPES.map((r) => (
              <button
                key={r.label}
                className="mtg-ask-chip"
                disabled={pending}
                onClick={() => void submit(r.prompt)}
              >
                {r.label}
              </button>
            ))}
          </div>
        )}

        {/* Input row (live controls take priority while recording) */}
        <div className="mtg-ask-row">
          {leading}
          {live ? (
            liveControls
          ) : (
            <>
              <input
                className="mtg-ask-input"
                type="text"
                placeholder="Ask anything"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void submit(question);
                  }
                }}
              />
              {pending ? (
                <span className="mtg-ask-pending" aria-label="Waiting for answer">
                  <Spinner />
                </span>
              ) : (
                <button
                  className="mtg-ask-send"
                  onClick={() => void submit(question)}
                  disabled={!question.trim()}
                  title="Ask"
                  aria-label="Ask"
                >
                  <ArrowUpIcon />
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
