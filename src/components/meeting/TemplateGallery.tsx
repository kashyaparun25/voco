import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { SUMMARY_TEMPLATES } from "./SummaryView";

/** User-defined summary template stored in the "custom_templates" setting. */
export interface CustomTemplate {
  id: string;
  name: string;
  emoji: string;
  instructions: string;
}

/** Unified card model (built-ins + customs). */
interface GalleryEntry {
  value: string;
  label: string;
  emoji: string;
  description: string;
  custom?: CustomTemplate;
}

interface TemplateGalleryProps {
  open: boolean;
  onClose: () => void;
  customTemplates: CustomTemplate[];
  favorites: string[];
  /** Currently selected template value (e.g. "standup" or "custom:<id>"). */
  selected: string;
  onSelect: (value: string) => void;
  onToggleFavorite: (value: string) => void;
  onEditCustom: (c: CustomTemplate) => void;
  onDeleteCustom: (c: CustomTemplate) => void;
  onNewTemplate: () => void;
}

/* ── Icons ─────────────────────────────────────────────────────────── */

const CloseIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
  </svg>
);

const StarIcon = ({ filled }: { filled: boolean }) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill={filled ? "currentColor" : "none"} viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M11.48 3.499a.562.562 0 0 1 1.04 0l2.125 5.111a.563.563 0 0 0 .475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 0 0-.182.557l1.285 5.385a.562.562 0 0 1-.84.61l-4.725-2.885a.562.562 0 0 0-.586 0L6.982 20.54a.562.562 0 0 1-.84-.61l1.285-5.386a.562.562 0 0 0-.182-.557l-4.204-3.602a.562.562 0 0 1 .321-.988l5.518-.442a.563.563 0 0 0 .475-.345L11.48 3.5Z" />
  </svg>
);

const PencilIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0 1 15.75 21H5.25A2.25 2.25 0 0 1 3 18.75V8.25A2.25 2.25 0 0 1 5.25 6H10" />
  </svg>
);

const TrashIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
  </svg>
);

const CheckIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2.2} stroke="currentColor" style={{ width: 13, height: 13 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
  </svg>
);

const PlusIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" style={{ width: 16, height: 16 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
  </svg>
);

/* ── Critical inline styles (same hardening rules as the dropdown:
      no cascade/@layer/global rule may break the box) ───────────────── */

/* Fixed full-screen flex wrapper does the centering: the panel must NOT be
   centered via its own `transform` — the open animation's keyframes also set
   `transform` (with fill `both`), which would override the translate and dump
   the panel's top-left at the viewport center (seen in the wild). */
const wrapperStyle: CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 1100,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: 24,
  boxSizing: "border-box",
};

const backdropStyle: CSSProperties = {
  position: "absolute",
  inset: 0,
  background: "rgba(0, 0, 0, 0.45)",
};

const panelStyle: CSSProperties = {
  position: "relative",
  width: "min(760px, calc(100vw - 48px))",
  maxHeight: "min(80vh, 100%)",
  zIndex: 1,
  transformOrigin: "center",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
  boxSizing: "border-box",
  margin: 0,
  padding: 0,
  borderRadius: 16,
  background: "var(--color-background-elevated, #1e1e2e)",
  backdropFilter: "blur(24px)",
  WebkitBackdropFilter: "blur(24px)",
  border: "1px solid var(--color-border-strong, rgba(255,255,255,0.12))",
  boxShadow: "0 24px 64px rgba(0, 0, 0, 0.45)",
};

export default function TemplateGallery({
  open,
  onClose,
  customTemplates,
  favorites,
  selected,
  onSelect,
  onToggleFavorite,
  onEditCustom,
  onDeleteCustom,
  onNewTemplate,
}: TemplateGalleryProps) {
  const [query, setQuery] = useState("");

  // Fresh search every time the gallery opens.
  useEffect(() => {
    if (open) setQuery("");
  }, [open]);

  const { favoriteEntries, customEntries, builtinEntries } = useMemo(() => {
    const customs: GalleryEntry[] = customTemplates.map((c) => ({
      value: `custom:${c.id}`,
      label: c.name,
      emoji: c.emoji || "📝",
      description: c.instructions || "Custom template",
      custom: c,
    }));
    const builtins: GalleryEntry[] = SUMMARY_TEMPLATES.map((t) => ({ ...t }));
    const all = [...builtins, ...customs];

    const q = query.trim().toLowerCase();
    const matches = (t: GalleryEntry) =>
      !q || t.label.toLowerCase().includes(q) || t.description.toLowerCase().includes(q);

    return {
      favoriteEntries: all.filter((t) => favorites.includes(t.value) && matches(t)),
      customEntries: customs.filter(matches),
      builtinEntries: builtins.filter(matches),
    };
  }, [customTemplates, favorites, query]);

  if (!open) return null;

  const card = (t: GalleryEntry) => {
    const isSelected = t.value === selected;
    const fav = favorites.includes(t.value);
    return (
      <div
        key={t.value}
        className={`mtg-gal-card${isSelected ? " mtg-gal-card-selected" : ""}`}
        role="button"
        tabIndex={0}
        onClick={() => onSelect(t.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onSelect(t.value);
          }
        }}
      >
        <div className="mtg-gal-card-top">
          <span className="mtg-gal-emoji" aria-hidden="true">{t.emoji}</span>
          <span className="mtg-gal-name">{t.label}</span>
          <span className="mtg-gal-card-actions">
            {isSelected && (
              <span className="mtg-gal-check" aria-label="Selected">
                <CheckIcon />
              </span>
            )}
            {t.custom && (
              <>
                <button
                  className="mtg-gal-cardbtn"
                  title="Edit template"
                  onClick={(e) => {
                    e.stopPropagation();
                    onEditCustom(t.custom!);
                  }}
                >
                  <PencilIcon />
                </button>
                <button
                  className="mtg-gal-cardbtn mtg-gal-trash"
                  title="Delete template"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteCustom(t.custom!);
                  }}
                >
                  <TrashIcon />
                </button>
              </>
            )}
            <button
              className={`mtg-gal-cardbtn mtg-gal-star${fav ? " mtg-gal-star-on" : ""}`}
              title={fav ? "Remove from favorites" : "Add to favorites"}
              aria-pressed={fav}
              onClick={(e) => {
                e.stopPropagation();
                onToggleFavorite(t.value);
              }}
            >
              <StarIcon filled={fav} />
            </button>
          </span>
        </div>
        <div className="mtg-gal-desc">{t.description}</div>
      </div>
    );
  };

  const searching = query.trim().length > 0;

  return createPortal(
    <div style={wrapperStyle}>
      <div style={backdropStyle} onClick={onClose} />
      <div
        className="mtg-gal-panel"
        style={panelStyle}
        role="dialog"
        aria-modal="true"
        aria-label="Template gallery"
      >
        {/* Header */}
        <div className="mtg-gal-head">
          <h2 className="mtg-gal-title mtg-serif">Templates</h2>
          <input
            className="mtg-gal-search"
            placeholder="Search templates"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
            spellCheck={false}
          />
          <button className="mtg-gal-close" title="Close" onClick={onClose}>
            <CloseIcon />
          </button>
        </div>

        {/* Scrollable body */}
        <div className="mtg-gal-body" style={{ overflowY: "auto", minHeight: 0 }}>
          {favoriteEntries.length > 0 && (
            <>
              <div className="mtg-gal-label">Favorites</div>
              <div className="mtg-gal-grid">{favoriteEntries.map(card)}</div>
            </>
          )}

          {(customEntries.length > 0 || !searching) && (
            <>
              <div className="mtg-gal-label">Your templates</div>
              <div className="mtg-gal-grid">
                {customEntries.map(card)}
                {!searching && (
                  <button className="mtg-gal-card mtg-gal-new" onClick={onNewTemplate}>
                    <span className="mtg-gal-new-icon">
                      <PlusIcon />
                    </span>
                    <span className="mtg-gal-name">New template</span>
                  </button>
                )}
              </div>
            </>
          )}

          <div className="mtg-gal-label">All templates</div>
          {builtinEntries.length > 0 ? (
            <div className="mtg-gal-grid">{builtinEntries.map(card)}</div>
          ) : (
            <div className="mtg-gal-empty">No templates match “{query.trim()}”.</div>
          )}
        </div>
      </div>
    </div>,
    document.getElementById("voco-theme-root") ?? document.body
  );
}
