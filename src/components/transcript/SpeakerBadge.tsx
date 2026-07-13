import React, { useState, useRef, useEffect } from "react";

interface SpeakerBadgeProps {
  speakerId: string;
  speakerName: string | null;
  onRenameSpeaker: (speakerId: string, newName: string) => void;
  style?: React.CSSProperties;
}

export default function SpeakerBadge({
  speakerId,
  speakerName,
  onRenameSpeaker,
  style
}: SpeakerBadgeProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState(speakerName || `Speaker ${speakerId}`);
  const inputRef = useRef<HTMLInputElement>(null);

  const getSpeakerColor = (id: string) => {
    let hash = 0;
    for (let i = 0; i < id.length; i++) {
      hash = id.charCodeAt(i) + ((hash << 5) - hash);
    }
    const index = Math.abs(hash % 8) + 1;
    return `var(--color-speaker-${index})`;
  };

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsEditing(true);
    setEditValue(speakerName || `Speaker ${speakerId}`);
  };

  const handleSave = () => {
    setIsEditing(false);
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== speakerName) {
      onRenameSpeaker(speakerId, trimmed);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSave();
    } else if (e.key === "Escape") {
      setIsEditing(false);
      setEditValue(speakerName || `Speaker ${speakerId}`);
    }
  };

  const badgeColor = getSpeakerColor(speakerId);

  if (isEditing) {
    return (
      <input
        ref={inputRef}
        type="text"
        value={editValue}
        onChange={(e) => setEditValue(e.target.value)}
        onBlur={handleSave}
        onKeyDown={handleKeyDown}
        style={{
          fontSize: "11px",
          fontWeight: "bold",
          padding: "2px 6px",
          borderRadius: "6px",
          border: `1px solid ${badgeColor}`,
          backgroundColor: "var(--color-background-elevated)",
          color: "var(--color-text-primary)",
          outline: "none",
          width: "120px",
          boxShadow: `0 0 4px ${badgeColor}33`,
          ...style
        }}
      />
    );
  }

  return (
    <div
      onDoubleClick={handleDoubleClick}
      title="Double-click to rename speaker"
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontSize: "11px",
        fontWeight: "bold",
        padding: "2px 8px",
        borderRadius: "6px",
        backgroundColor: `${badgeColor}15`,
        color: badgeColor,
        border: `1px solid ${badgeColor}40`,
        cursor: "pointer",
        userSelect: "none",
        transition: "all 0.15s ease",
        ...style
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.backgroundColor = `${badgeColor}25`;
        e.currentTarget.style.transform = "translateY(-1px)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.backgroundColor = `${badgeColor}15`;
        e.currentTarget.style.transform = "none";
      }}
    >
      {speakerName || `Speaker ${speakerId}`}
    </div>
  );
}
