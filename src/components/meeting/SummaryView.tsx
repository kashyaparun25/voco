import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "@astryxdesign/core/Card";
import { Button } from "../ui";
import { Text } from "@astryxdesign/core/Text";
import { VStack, HStack } from "@astryxdesign/core/Layout";

// Types for settings
export type SummaryLength = "short" | "medium" | "long";
export type SummaryStyle = "bullets" | "paragraphs" | "action";

// Meetily-style structured summary templates.
const SUMMARY_TEMPLATES = [
  { value: "general", label: "General" },
  { value: "standup", label: "Standup" },
  { value: "one_on_one", label: "1:1" },
  { value: "sales", label: "Sales Call" },
  { value: "interview", label: "Interview" },
  { value: "retrospective", label: "Retrospective" },
  { value: "decision_log", label: "Decision Log" },
];

interface SummaryViewProps {
  meetingId: string;
  meetingTitle: string;
  summary: string | null;
  isLoading: boolean;
  /** Live text streamed from `summary-token` events while generating. */
  streamingText?: string;
  /** True while a token stream is in-flight. */
  isStreaming?: boolean;
  onGenerate: (length: SummaryLength, style: SummaryStyle) => void;
  onRegenerate: (length: SummaryLength, style: SummaryStyle) => void;
  style?: React.CSSProperties;
}

// Inline styles for animations
const animationsStyle = `
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: .5; }
}
`;

// Icons
const SparklesIcon = ({ size = 16, color = "currentColor" }: { size?: number; color?: string }) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke={color} style={{ width: size, height: size }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 0 0-2.456 2.456Z" />
  </svg>
);

const CopyIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H5.25m11.9-3.664A2.251 2.251 0 0 0 15 2.25h-1.5a2.25 2.25 0 0 0-2.25 2.25h-.375c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V7.875c0-.621-.504-1.125-1.125-1.125H18a2.25 2.25 0 0 0-2.25-2.25Z" />
  </svg>
);

const CheckIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="#10b981" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
  </svg>
);

const DownloadIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" style={{ width: 14, height: 14 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" />
  </svg>
);

const ChevronDownIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.8} stroke="currentColor" style={{ width: 12, height: 12 }}>
    <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
  </svg>
);

const SpinnerIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" style={{ width: 16, height: 16, animation: "spin 1s linear infinite" }}>
    <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" style={{ opacity: 0.25 }} />
    <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" style={{ opacity: 0.75 }} />
  </svg>
);

const isTableRow = (l: string) => /^\s*\|.*\|\s*$/.test(l);
const isTableSep = (l: string) => /^\s*\|?[\s:-]*-{2,}[\s:|-]*\|?\s*$/.test(l) && l.includes("-");
const splitRow = (l: string) =>
  l.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());

function renderTable(header: string[], rows: string[][], key: React.Key): React.ReactNode {
  const cellBase: React.CSSProperties = {
    border: "1px solid var(--color-border)",
    padding: "7px 10px",
    fontSize: "12.5px",
    lineHeight: 1.5,
    textAlign: "left",
    verticalAlign: "top",
    color: "var(--color-text-primary)",
  };
  return (
    <div key={key} style={{ overflowX: "auto", margin: "10px 0" }}>
      <table style={{ borderCollapse: "collapse", width: "100%", minWidth: header.length * 90 }}>
        <thead>
          <tr>
            {header.map((h, i) => (
              <th key={i} style={{ ...cellBase, fontWeight: 700, background: "var(--color-background-surface-hover)" }}>
                {renderTextWithBold(h)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, ri) => (
            <tr key={ri}>
              {header.map((_, ci) => (
                <td key={ci} style={cellBase}>{renderTextWithBold(r[ci] ?? "")}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// Render a useful subset of Markdown: headings, bullets, numbered lists, **bold**,
// and GitHub-flavored pipe tables (for action items / structured rows).
function parseMarkdown(text: string): React.ReactNode[] {
  if (!text) return [];
  const lines = text.split("\n");
  const out: React.ReactNode[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // Table block: a row followed by a `| --- |` separator.
    if (isTableRow(line) && i + 1 < lines.length && isTableSep(lines[i + 1])) {
      const header = splitRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && isTableRow(lines[i]) && !isTableSep(lines[i])) {
        rows.push(splitRow(lines[i]));
        i++;
      }
      out.push(renderTable(header, rows, `t-${i}`));
      continue;
    }

    if (line.startsWith("### ")) {
      out.push(<h3 key={i} style={{ fontSize: "14px", fontWeight: 700, margin: "14px 0 6px", color: "var(--color-text-primary)", letterSpacing: "-0.01em" }}>{renderTextWithBold(line.slice(4))}</h3>);
    } else if (line.startsWith("## ")) {
      out.push(<h2 key={i} style={{ fontSize: "16px", fontWeight: 700, margin: "18px 0 8px", color: "var(--color-text-primary)", letterSpacing: "-0.015em" }}>{renderTextWithBold(line.slice(3))}</h2>);
    } else if (line.startsWith("# ")) {
      out.push(<h1 key={i} style={{ fontSize: "18px", fontWeight: 700, margin: "22px 0 12px", color: "var(--color-text-primary)", letterSpacing: "-0.02em" }}>{renderTextWithBold(line.slice(2))}</h1>);
    } else if (/^\s*[-*+]\s+/.test(line)) {
      out.push(<li key={i} style={{ marginLeft: 18, marginBottom: 6, color: "var(--color-text-primary)", fontSize: "13px", lineHeight: 1.6, listStyleType: "disc" }}>{renderTextWithBold(line.replace(/^\s*[-*+]\s+/, ""))}</li>);
    } else if (/^\s*\d+\.\s+/.test(line)) {
      out.push(<div key={i} style={{ marginLeft: 6, marginBottom: 6, color: "var(--color-text-primary)", fontSize: "13px", lineHeight: 1.6 }}>{renderTextWithBold(line.trim())}</div>);
    } else if (line.trim() === "") {
      out.push(<div key={i} style={{ height: "6px" }} />);
    } else {
      out.push(<p key={i} style={{ margin: "6px 0", color: "var(--color-text-primary)", fontSize: "13px", lineHeight: 1.6 }}>{renderTextWithBold(line)}</p>);
    }
    i++;
  }
  return out;
}

function renderTextWithBold(text: string): React.ReactNode {
  // Bold parser matching **bold text**
  const parts = text.split(/(\*\*.*?\*\*)/g);
  return parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={i} style={{ fontWeight: "700", color: "var(--color-text-primary)" }}>{part.slice(2, -2)}</strong>;
    }
    return part;
  });
}

export default function SummaryView({
  meetingTitle,
  summary,
  isLoading,
  streamingText,
  isStreaming,
  onGenerate,
  onRegenerate,
  style
}: SummaryViewProps) {
  // Dropdown & Popover toggles
  const [isRegenerateOpen, setIsRegenerateOpen] = useState(false);
  const [isExportOpen, setIsExportOpen] = useState(false);
  
  // Customization settings
  const [selectedLength, setSelectedLength] = useState<SummaryLength>("medium");
  const [selectedStyle, setSelectedStyle] = useState<SummaryStyle>("bullets");
  const [selectedTemplate, setSelectedTemplate] = useState<string>("general");

  // Copy feedback state
  const [copied, setCopied] = useState(false);

  // Load the persisted template (backend reads `summary_template` at generate time).
  useEffect(() => {
    (async () => {
      try {
        const t = await invoke<string | null>("get_setting", { key: "summary_template" });
        if (t) setSelectedTemplate(t);
      } catch { /* ignore */ }
    })();
  }, []);

  const changeTemplate = (value: string) => {
    setSelectedTemplate(value);
    void invoke("set_setting", { key: "summary_template", value }).catch(() => {});
  };

  const getTemplateLabel = (v: string) =>
    SUMMARY_TEMPLATES.find((t) => t.value === v)?.label ?? "General";

  const handleCopy = async () => {
    if (!summary) return;
    try {
      await navigator.clipboard.writeText(summary);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy summary to clipboard", err);
    }
  };

  const handleExport = (format: "md" | "txt") => {
    if (!summary) return;
    setIsExportOpen(false);
    
    const blob = new Blob([summary], { type: format === "md" ? "text/markdown" : "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    
    // Format title for filename
    const cleanTitle = meetingTitle.toLowerCase().trim().replace(/[^a-z0-9]+/g, "_");
    a.download = `${cleanTitle}_summary.${format}`;
    
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const getStyleLabel = (s: SummaryStyle) => {
    switch (s) {
      case "bullets": return "Bullet Points";
      case "paragraphs": return "Paragraphs";
      case "action": return "Action-Oriented";
    }
  };

  const getLengthLabel = (l: SummaryLength) => {
    switch (l) {
      case "short": return "Short";
      case "medium": return "Medium";
      case "long": return "Long";
    }
  };

  // Render live streaming tokens (before falling back to the skeleton).
  if ((isLoading || isStreaming) && streamingText && streamingText.length > 0) {
    return (
      <Card style={{
        padding: "20px",
        backgroundColor: "var(--color-background-surface, #1e1e2f)",
        border: "1px solid var(--color-border, #2d2d3f)",
        borderRadius: "14px",
        display: "flex",
        flexDirection: "column",
        gap: "14px",
        boxShadow: "0 4px 20px rgba(0, 0, 0, 0.05)",
        ...style
      }}>
        <style dangerouslySetInnerHTML={{ __html: animationsStyle }} />
        <HStack gap={2} style={{ alignItems: "center", color: "var(--color-accent)" }}>
          <SpinnerIcon />
          <Text style={{ fontSize: "14px", fontWeight: "700", letterSpacing: "-0.01em" }}>
            AI is writing your summary...
          </Text>
        </HStack>
        <div style={{
          backgroundColor: "rgba(0, 0, 0, 0.08)",
          borderRadius: "8px",
          padding: "12px 14px",
          maxHeight: "300px",
          overflowY: "auto",
          border: "1px solid rgba(255, 255, 255, 0.02)"
        }}>
          {parseMarkdown(streamingText)}
          <span style={{
            display: "inline-block",
            width: "7px",
            height: "14px",
            marginLeft: "2px",
            backgroundColor: "var(--color-accent)",
            verticalAlign: "text-bottom",
            animation: "pulse 1s ease-in-out infinite"
          }} />
        </div>
      </Card>
    );
  }

  // Render Skeleton Loader
  if (isLoading) {
    return (
      <Card style={{
        padding: "20px",
        backgroundColor: "var(--color-background-surface, #1e1e2f)",
        border: "1px solid var(--color-border, #2d2d3f)",
        borderRadius: "14px",
        display: "flex",
        flexDirection: "column",
        gap: "14px",
        boxShadow: "0 4px 20px rgba(0, 0, 0, 0.05)",
        ...style
      }}>
        <style dangerouslySetInnerHTML={{ __html: animationsStyle }} />
        <HStack style={{ justifyContent: "space-between", alignItems: "center" }}>
          <HStack gap={2} style={{ alignItems: "center", color: "var(--color-accent)" }}>
            <SpinnerIcon />
            <Text style={{ fontSize: "14px", fontWeight: "700", letterSpacing: "-0.01em" }}>
              AI is writing your summary...
            </Text>
          </HStack>
        </HStack>
        <VStack gap={3} style={{ width: "100%", marginTop: "4px" }}>
          <div style={{
            height: "14px",
            backgroundColor: "rgba(124, 58, 237, 0.08)",
            borderRadius: "4px",
            width: "85%",
            animation: "pulse 1.5s ease-in-out infinite"
          }} />
          <div style={{
            height: "14px",
            backgroundColor: "rgba(124, 58, 237, 0.08)",
            borderRadius: "4px",
            width: "95%",
            animation: "pulse 1.5s ease-in-out infinite",
            animationDelay: "0.2s"
          }} />
          <div style={{
            height: "14px",
            backgroundColor: "rgba(124, 58, 237, 0.08)",
            borderRadius: "4px",
            width: "60%",
            animation: "pulse 1.5s ease-in-out infinite",
            animationDelay: "0.4s"
          }} />
        </VStack>
      </Card>
    );
  }

  // Render Empty State (No summary yet)
  if (!summary) {
    return (
      <Card style={{
        padding: "24px",
        backgroundColor: "rgba(124, 58, 237, 0.02)",
        border: "1px dashed rgba(124, 58, 237, 0.3)",
        borderRadius: "14px",
        display: "flex",
        flexDirection: "column",
        gap: "16px",
        alignItems: "center",
        textAlign: "center",
        ...style
      }}>
        <div style={{
          width: "48px",
          height: "48px",
          borderRadius: "50%",
          backgroundColor: "rgba(124, 58, 237, 0.08)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "var(--color-accent)",
          marginBottom: "4px"
        }}>
          <SparklesIcon size={22} color="var(--color-accent)" />
        </div>

        <VStack gap={1} style={{ alignItems: "center" }}>
          <Text style={{ fontSize: "15px", fontWeight: "700", color: "var(--color-text-primary)" }}>
            Generate AI Meeting Summary
          </Text>
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)", maxWidth: "340px", lineHeight: "1.4" }}>
            Get instant key decisions, action items, and structural takeaways from your meeting transcript using local LLMs.
          </Text>
        </VStack>

        {/* Template selection */}
        <VStack gap={1} style={{ width: "100%", maxWidth: "380px", alignItems: "flex-start" }}>
          <Text style={{ fontSize: "11px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Template</Text>
          <select
            value={selectedTemplate}
            onChange={(e) => changeTemplate(e.target.value)}
            style={{
              width: "100%",
              padding: "8px 10px",
              borderRadius: "6px",
              backgroundColor: "var(--color-background-elevated)",
              color: "var(--color-text-primary)",
              border: "1px solid var(--color-border-strong)",
              fontSize: "12px",
              outline: "none",
              cursor: "pointer",
            }}
          >
            {SUMMARY_TEMPLATES.map((t) => (
              <option key={t.value} value={t.value}>{t.label}</option>
            ))}
          </select>
        </VStack>

        <HStack gap={4} style={{ width: "100%", maxWidth: "380px", marginTop: "4px" }}>
          {/* Length selection */}
          <VStack gap={1} style={{ flex: 1, alignItems: "flex-start" }}>
            <Text style={{ fontSize: "11px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Length</Text>
            <select
              value={selectedLength}
              onChange={(e) => setSelectedLength(e.target.value as SummaryLength)}
              style={{
                width: "100%",
                padding: "8px 10px",
                borderRadius: "6px",
                backgroundColor: "var(--color-background-surface, #212130)",
                color: "var(--color-text-primary)",
                border: "1px solid var(--color-border)",
                fontSize: "12px",
                outline: "none",
                cursor: "pointer"
              }}
            >
              <option value="short">Short</option>
              <option value="medium">Medium</option>
              <option value="long">Long</option>
            </select>
          </VStack>

          {/* Style selection */}
          <VStack gap={1} style={{ flex: 1, alignItems: "flex-start" }}>
            <Text style={{ fontSize: "11px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Format Style</Text>
            <select
              value={selectedStyle}
              onChange={(e) => setSelectedStyle(e.target.value as SummaryStyle)}
              style={{
                width: "100%",
                padding: "8px 10px",
                borderRadius: "6px",
                backgroundColor: "var(--color-background-surface, #212130)",
                color: "var(--color-text-primary)",
                border: "1px solid var(--color-border)",
                fontSize: "12px",
                outline: "none",
                cursor: "pointer"
              }}
            >
              <option value="bullets">Bullet Points</option>
              <option value="paragraphs">Paragraphs</option>
              <option value="action">Action-Oriented</option>
            </select>
          </VStack>
        </HStack>

        <Button
          variant="primary"
          onClick={() => onGenerate(selectedLength, selectedStyle)}
          label="Generate Summary"
          icon={<SparklesIcon size={14} color="#ffffff" />}
          style={{
            marginTop: "8px",
            padding: "8px 20px",
            borderRadius: "8px",
            backgroundColor: "var(--color-accent)",
            color: "#ffffff",
            fontWeight: "600",
            fontSize: "13px",
            border: "none",
            cursor: "pointer",
            boxShadow: "0 4px 12px rgba(124, 58, 237, 0.2)"
          }}
        />
      </Card>
    );
  }

  // Render Full summary details
  return (
    <Card style={{
      padding: "20px",
      backgroundColor: "var(--color-background-surface, #1e1e2f)",
      border: "1px solid var(--color-border, #2d2d3f)",
      borderRadius: "14px",
      display: "flex",
      flexDirection: "column",
      gap: "14px",
      position: "relative",
      boxShadow: "0 4px 20px rgba(0, 0, 0, 0.05)",
      ...style
    }}>
      <style dangerouslySetInnerHTML={{ __html: animationsStyle }} />
      
      {/* Header bar */}
      <HStack style={{ justifyContent: "space-between", alignItems: "center", width: "100%" }}>
        <HStack gap={2} style={{ alignItems: "center", color: "var(--color-accent)" }}>
          <SparklesIcon size={18} color="var(--color-accent)" />
          <Text style={{ fontSize: "14px", fontWeight: "700", color: "var(--color-accent-text, var(--color-accent))", letterSpacing: "-0.01em" }}>
            AI Meeting Summary
          </Text>
          <span style={{
            fontSize: "10px",
            backgroundColor: "rgba(124, 58, 237, 0.08)",
            color: "var(--color-accent)",
            padding: "2px 6px",
            borderRadius: "4px",
            fontWeight: "600",
            marginLeft: "4px"
          }}>
            {getTemplateLabel(selectedTemplate)} • {getLengthLabel(selectedLength)} • {getStyleLabel(selectedStyle)}
          </span>
        </HStack>

        {/* Action buttons */}
        <HStack gap={2} style={{ alignItems: "center", position: "relative" }}>
          
          {/* Copy Button */}
          <button
            onClick={handleCopy}
            title="Copy to Clipboard"
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "28px",
              height: "28px",
              borderRadius: "6px",
              backgroundColor: "var(--color-background-elevated, #2d2d3f)",
              border: "1px solid var(--color-border-strong)",
              color: "var(--color-text-primary)",
              cursor: "pointer",
              transition: "all 0.15s ease"
            }}
            onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)"}
            onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-elevated)"}
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
          </button>

          {/* Export Button & Dropdown */}
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setIsExportOpen(!isExportOpen)}
              title="Export Summary"
              style={{
                display: "flex",
                alignItems: "center",
                gap: "4px",
                padding: "0 8px",
                height: "28px",
                borderRadius: "6px",
                backgroundColor: "var(--color-background-elevated, #2d2d3f)",
                border: "1px solid var(--color-border-strong)",
                color: "var(--color-text-primary)",
                fontSize: "12px",
                fontWeight: "500",
                cursor: "pointer",
                transition: "all 0.15s ease"
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)"}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-elevated)"}
            >
              <DownloadIcon />
              <span>Export</span>
              <ChevronDownIcon />
            </button>

            {isExportOpen && (
              <>
                <div
                  onClick={() => setIsExportOpen(false)}
                  style={{
                    position: "fixed",
                    top: 0,
                    left: 0,
                    right: 0,
                    bottom: 0,
                    zIndex: 99
                  }}
                />
                <div style={{
                  position: "absolute",
                  right: 0,
                  top: "32px",
                  zIndex: 100,
                  width: "140px",
                  backgroundColor: "var(--color-background-elevated, #212130)",
                  border: "1px solid var(--color-border-strong, #3b3b4f)",
                  borderRadius: "8px",
                  boxShadow: "0 6px 16px rgba(0,0,0,0.3)",
                  padding: "4px",
                  display: "flex",
                  flexDirection: "column"
                }}>
                  <button
                    onClick={() => handleExport("md")}
                    style={{
                      padding: "8px 12px",
                      textAlign: "left",
                      backgroundColor: "transparent",
                      border: "none",
                      color: "var(--color-text-primary)",
                      fontSize: "12px",
                      cursor: "pointer",
                      borderRadius: "4px",
                      transition: "background 0.15s"
                    }}
                    onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)"}
                    onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "transparent"}
                  >
                    Markdown (.md)
                  </button>
                  <button
                    onClick={() => handleExport("txt")}
                    style={{
                      padding: "8px 12px",
                      textAlign: "left",
                      backgroundColor: "transparent",
                      border: "none",
                      color: "var(--color-text-primary)",
                      fontSize: "12px",
                      cursor: "pointer",
                      borderRadius: "4px",
                      transition: "background 0.15s"
                    }}
                    onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)"}
                    onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "transparent"}
                  >
                    Plain Text (.txt)
                  </button>
                </div>
              </>
            )}
          </div>

          {/* Regenerate Button & Popover */}
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setIsRegenerateOpen(!isRegenerateOpen)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "4px",
                padding: "0 8px",
                height: "28px",
                borderRadius: "6px",
                backgroundColor: "var(--color-background-elevated)",
                border: "1px solid var(--color-border-strong)",
                color: "var(--color-text-primary)",
                fontSize: "12px",
                fontWeight: "500",
                cursor: "pointer",
                transition: "all 0.15s ease"
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-surface-hover)"}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "var(--color-background-elevated)"}
            >
              <span>Options</span>
              <ChevronDownIcon />
            </button>

            {isRegenerateOpen && (
              <>
                <div
                  onClick={() => setIsRegenerateOpen(false)}
                  style={{
                    position: "fixed",
                    top: 0,
                    left: 0,
                    right: 0,
                    bottom: 0,
                    zIndex: 99
                  }}
                />
                <div style={{
                  position: "absolute",
                  right: 0,
                  top: "32px",
                  zIndex: 100,
                  width: "250px",
                  backgroundColor: "var(--color-background-elevated, #212130)",
                  border: "1px solid var(--color-border-strong, #3b3b4f)",
                  borderRadius: "10px",
                  boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
                  padding: "14px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "10px"
                }}>
                  <Text style={{ fontSize: "12px", fontWeight: "700", color: "var(--color-text-primary)", marginBottom: "2px" }}>
                    Regeneration Settings
                  </Text>

                  <VStack gap={1} style={{ alignItems: "flex-start" }}>
                    <Text style={{ fontSize: "10px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Template</Text>
                    <select
                      value={selectedTemplate}
                      onChange={(e) => changeTemplate(e.target.value)}
                      style={{
                        width: "100%",
                        padding: "6px 8px",
                        borderRadius: "4px",
                        backgroundColor: "var(--color-background-surface)",
                        color: "var(--color-text-primary)",
                        border: "1px solid var(--color-border-strong)",
                        fontSize: "11px",
                        outline: "none",
                        cursor: "pointer",
                      }}
                    >
                      {SUMMARY_TEMPLATES.map((t) => (
                        <option key={t.value} value={t.value}>{t.label}</option>
                      ))}
                    </select>
                  </VStack>

                  <VStack gap={1} style={{ alignItems: "flex-start" }}>
                    <Text style={{ fontSize: "10px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Length</Text>
                    <select
                      value={selectedLength}
                      onChange={(e) => setSelectedLength(e.target.value as SummaryLength)}
                      style={{
                        width: "100%",
                        padding: "6px 8px",
                        borderRadius: "4px",
                        backgroundColor: "var(--color-background-surface, #1e1e2f)",
                        color: "var(--color-text-primary)",
                        border: "1px solid var(--color-border-strong)",
                        fontSize: "11px",
                        outline: "none",
                        cursor: "pointer"
                      }}
                    >
                      <option value="short">Short</option>
                      <option value="medium">Medium (Balanced)</option>
                      <option value="long">Long (Detailed)</option>
                    </select>
                  </VStack>

                  <VStack gap={1} style={{ alignItems: "flex-start" }}>
                    <Text style={{ fontSize: "10px", fontWeight: "600", color: "var(--color-text-secondary)" }}>Format Style</Text>
                    <select
                      value={selectedStyle}
                      onChange={(e) => setSelectedStyle(e.target.value as SummaryStyle)}
                      style={{
                        width: "100%",
                        padding: "6px 8px",
                        borderRadius: "4px",
                        backgroundColor: "var(--color-background-surface, #1e1e2f)",
                        color: "var(--color-text-primary)",
                        border: "1px solid var(--color-border-strong)",
                        fontSize: "11px",
                        outline: "none",
                        cursor: "pointer"
                      }}
                    >
                      <option value="bullets">Bullet Points</option>
                      <option value="paragraphs">Paragraphs</option>
                      <option value="action">Action-Oriented</option>
                    </select>
                  </VStack>

                  <button
                    onClick={() => {
                      setIsRegenerateOpen(false);
                      onRegenerate(selectedLength, selectedStyle);
                    }}
                    style={{
                      marginTop: "6px",
                      width: "100%",
                      padding: "8px",
                      borderRadius: "6px",
                      backgroundColor: "var(--color-accent)",
                      color: "#ffffff",
                      border: "none",
                      fontSize: "12px",
                      fontWeight: "600",
                      cursor: "pointer",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      gap: "4px"
                    }}
                  >
                    <SparklesIcon size={12} color="#ffffff" />
                    <span>Regenerate Summary</span>
                  </button>
                </div>
              </>
            )}
          </div>

        </HStack>
      </HStack>

      {/* Render parsed markdown text */}
      <div style={{
        backgroundColor: "rgba(0, 0, 0, 0.08)",
        borderRadius: "8px",
        padding: "12px 14px",
        maxHeight: "300px",
        overflowY: "auto",
        border: "1px solid rgba(255, 255, 255, 0.02)"
      }}>
        {parseMarkdown(summary)}
      </div>
    </Card>
  );
}
