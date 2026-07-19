import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Toggle } from "../ui";

type McpStatus = {
  enabled: boolean;
  sidecar_path: string;
  sidecar_exists: boolean;
  db_path: string;
  meeting_count: number;
  dictation_count: number;
};

type McpSetup = {
  claude_code_cmd: string;
  cursor_json: string;
  generic_json: string;
  cursor_deeplink: string;
  setup_prompt: string;
};

type TestResult = { ok: boolean; message: string };

const subheadStyle: React.CSSProperties = {
  fontSize: "15px",
  fontWeight: 700,
  color: "var(--color-text-primary)",
};

const cardStyle: React.CSSProperties = {
  backgroundColor: "var(--color-background-elevated)",
  border: "1px solid var(--color-border-strong)",
  borderRadius: 10,
  padding: 14,
};

const preStyle: React.CSSProperties = {
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  fontSize: 12,
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, 'Cascadia Mono', monospace",
  backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-secondary)",
  maxHeight: 140,
  overflow: "auto",
  padding: 10,
  borderRadius: 8,
  margin: 0,
  border: "1px solid var(--color-border)",
};

const btnStyle: React.CSSProperties = {
  padding: "6px 12px",
  borderRadius: 8,
  background: "transparent",
  color: "var(--color-text-secondary)",
  border: "1px solid var(--color-border-strong)",
  fontSize: 12,
  fontWeight: 600,
  fontFamily: "inherit",
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const monoMutedStyle: React.CSSProperties = {
  fontSize: 12,
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, 'Cascadia Mono', monospace",
  color: "var(--color-text-secondary)",
  wordBreak: "break-all",
};

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 520 }}>
      <VStack gap={1} style={{ flex: 1 }}>
        <Text style={{ fontSize: "14px", fontWeight: "600", color: "var(--color-text-primary)" }}>{label}</Text>
        {description ? (
          <Text style={{ fontSize: "12px", color: "var(--color-text-secondary)" }}>{description}</Text>
        ) : null}
      </VStack>
      <Toggle checked={checked} onChange={onChange} />
    </HStack>
  );
}

function Dot({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        backgroundColor: color,
        flexShrink: 0,
      }}
    />
  );
}

function SetupCard({
  title,
  description,
  code,
  extra,
}: {
  title: string;
  description: string;
  code: string;
  extra?: React.ReactNode;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (err) {
      console.error(`IntegrationsSettings: failed to copy ${title}:`, err);
    }
  };

  return (
    <div style={cardStyle}>
      <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", gap: 10 }}>
        <VStack gap={1} style={{ flex: 1, minWidth: 0 }}>
          <Text style={{ fontSize: 14, fontWeight: 700, color: "var(--color-text-primary)" }}>{title}</Text>
          <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{description}</Text>
        </VStack>
        <button style={btnStyle} onClick={() => void copy()}>
          {copied ? "Copied ✓" : "Copy"}
        </button>
      </HStack>
      <pre style={{ ...preStyle, marginTop: 10 }}>{code}</pre>
      {extra ? <div style={{ marginTop: 10 }}>{extra}</div> : null}
    </div>
  );
}

export default function IntegrationsSettings() {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [setup, setSetup] = useState<McpSetup | null>(null);
  const [backendAvailable, setBackendAvailable] = useState(true);
  const [enabled, setEnabled] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [promptCopied, setPromptCopied] = useState(false);

  const loadStatus = async () => {
    try {
      const s = await invoke<McpStatus>("mcp_get_status");
      setStatus(s);
      setEnabled(s.enabled);
      setBackendAvailable(true);
    } catch (err) {
      console.warn("IntegrationsSettings: mcp_get_status unavailable:", err);
      setBackendAvailable(false);
    }
  };

  useEffect(() => {
    const load = async () => {
      await loadStatus();
      try {
        const c = await invoke<McpSetup>("mcp_get_setup");
        setSetup(c);
      } catch (err) {
        console.warn("IntegrationsSettings: mcp_get_setup unavailable:", err);
      }
    };
    void load();
  }, []);

  const onToggle = () => {
    const next = !enabled;
    setEnabled(next);
    setStatus((prev) => (prev ? { ...prev, enabled: next } : prev));
    invoke("mcp_set_enabled", { enabled: next }).catch((err) => {
      console.warn("IntegrationsSettings: mcp_set_enabled failed:", err);
    });
  };

  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const r = await invoke<TestResult>("mcp_test_connection");
      setTestResult(r);
    } catch (err) {
      setTestResult({ ok: false, message: String(err) });
    } finally {
      setTesting(false);
    }
  };

  const copyPrompt = async () => {
    if (!setup) return;
    try {
      await navigator.clipboard.writeText(setup.setup_prompt);
      setPromptCopied(true);
      window.setTimeout(() => setPromptCopied(false), 1500);
    } catch (err) {
      console.error("IntegrationsSettings: failed to copy setup prompt:", err);
    }
  };

  return (
    <VStack gap={5} style={{ width: "100%" }}>
      <Text style={{ fontSize: "14px", color: "var(--color-text-secondary)" }}>
        Expose your meetings, transcripts, and dictations to coding agents (Claude Code, Cursor, and
        others) through a local MCP server. Everything stays on this machine.
      </Text>

      {!backendAvailable && (
        <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
          Backend unavailable — rebuild the app.
        </Text>
      )}

      {/* Enable */}
      <VStack gap={3} style={{ width: "100%" }}>
        <Text style={subheadStyle}>Model Context Protocol Server</Text>
        <ToggleRow
          label="Enable MCP server"
          description="Let coding agents read your meeting notes and dictation history on demand. Off by default."
          checked={enabled}
          onChange={onToggle}
        />

        {enabled && status && (
          <VStack gap={2} style={{ maxWidth: 520 }}>
            <HStack gap={2} style={{ alignItems: "center" }}>
              <Dot color={status.sidecar_exists ? "#3fb950" : "#d29922"} />
              <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
                {status.sidecar_exists
                  ? "Server binary ready"
                  : "Sidecar not found — rebuild or reinstall Voco."}
              </Text>
            </HStack>
            <Text style={monoMutedStyle}>{status.sidecar_path}</Text>
            <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
              {status.meeting_count} meetings · {status.dictation_count} dictations visible
            </Text>
          </VStack>
        )}
      </VStack>

      {!enabled && (
        <Text style={{ fontSize: 13, color: "var(--color-text-secondary)" }}>
          Enable the server above to get connection instructions.
        </Text>
      )}

      {enabled && setup && (
        <>
          {/* Connect a client */}
          <VStack gap={3} style={{ width: "100%" }}>
            <Text style={subheadStyle}>Connect a client</Text>
            <SetupCard
              title="Claude Code"
              description="Run this in your terminal."
              code={setup.claude_code_cmd}
            />
            <SetupCard
              title="Cursor"
              description="Add to ~/.cursor/mcp.json, or use the one-click button."
              code={setup.cursor_json}
              extra={
                <button style={btnStyle} onClick={() => window.open(setup.cursor_deeplink)}>
                  Add to Cursor
                </button>
              }
            />
            <SetupCard
              title="Other agents (Windsurf, Codex, Zed…)"
              description="Add this stdio server entry to your agent's MCP config."
              code={setup.generic_json}
            />
          </VStack>

          {/* Setup prompt */}
          <VStack gap={3} style={{ width: "100%" }}>
            <Text style={subheadStyle}>Or let your agent set itself up</Text>
            <div style={{ ...cardStyle, borderColor: "var(--color-accent)" }}>
              <HStack style={{ justifyContent: "space-between", alignItems: "flex-start", gap: 10 }}>
                <Text style={{ fontSize: 12, color: "var(--color-text-secondary)", flex: 1 }}>
                  Paste this into any coding agent and it will register Voco itself, then verify the
                  connection.
                </Text>
                <button style={btnStyle} onClick={() => void copyPrompt()}>
                  {promptCopied ? "Copied ✓" : "Copy setup prompt"}
                </button>
              </HStack>
              <pre style={{ ...preStyle, marginTop: 10 }}>{setup.setup_prompt}</pre>
            </div>
          </VStack>

          {/* Test */}
          <VStack gap={3} style={{ width: "100%" }}>
            <Text style={subheadStyle}>Test</Text>
            <HStack gap={2} style={{ alignItems: "center", flexWrap: "wrap" }}>
              <button
                style={{ ...btnStyle, opacity: testing ? 0.6 : 1 }}
                onClick={() => void runTest()}
                disabled={testing}
              >
                {testing ? "Testing…" : "Test connection"}
              </button>
              <button
                style={{ ...btnStyle, border: "none", padding: "6px 4px" }}
                onClick={() => void loadStatus()}
              >
                Refresh status
              </button>
            </HStack>
            {testResult && (
              <HStack gap={2} style={{ alignItems: "center" }}>
                <Dot color={testResult.ok ? "#3fb950" : "#f85149"} />
                <Text
                  style={{
                    fontSize: 13,
                    color: testResult.ok ? "#3fb950" : "#f85149",
                  }}
                >
                  {testResult.ok ? "✓ " : "✕ "}
                  {testResult.message}
                </Text>
              </HStack>
            )}
          </VStack>
        </>
      )}
    </VStack>
  );
}
