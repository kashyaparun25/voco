import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";
import { Button, Toggle, TextInput } from "../ui";
import { showToast } from "../../hooks/useToast";

interface GoogleStatus {
  connected: boolean;
  email: string | null;
  has_credentials: boolean;
}

const labelStyle: React.CSSProperties = { fontSize: 13, fontWeight: 600, color: "var(--color-text-secondary)" };
const selectStyle: React.CSSProperties = {
  padding: "8px 12px", borderRadius: 8, backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-primary)", border: "1px solid var(--color-border-strong)", fontSize: 14, outline: "none", width: 90,
};

function ToggleRow({ label, description, checked, onChange }: { label: string; description?: string; checked: boolean; onChange: () => void }) {
  return (
    <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 520 }}>
      <VStack gap={1} style={{ flex: 1 }}>
        <Text style={{ fontSize: 14, fontWeight: 600, color: "var(--color-text-primary)" }}>{label}</Text>
        {description ? <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>{description}</Text> : null}
      </VStack>
      <Toggle checked={checked} onChange={onChange} />
    </HStack>
  );
}

export default function GoogleCalendarSettings() {
  const [status, setStatus] = useState<GoogleStatus>({ connected: false, email: null, has_credentials: false });
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [editingCreds, setEditingCreds] = useState(false);
  const [signingIn, setSigningIn] = useState(false);

  const [notify, setNotify] = useState(false);
  const [autoStart, setAutoStart] = useState(false);
  const [minsBefore, setMinsBefore] = useState("1");

  const refresh = async () => {
    try { setStatus(await invoke<GoogleStatus>("google_status")); } catch { /* ignore */ }
  };

  useEffect(() => {
    void refresh();
    (async () => {
      const g = async (k: string) => { try { return await invoke<string | null>("get_setting", { key: k }); } catch { return null; } };
      const n = await g("meeting_notify_enabled"); if (n != null) setNotify(n === "true");
      const a = await g("meeting_autostart_enabled"); if (a != null) setAutoStart(a === "true");
      const m = await g("meeting_notify_before_min"); if (m) setMinsBefore(m);
    })();
  }, []);

  const persist = (key: string, value: string) => void invoke("set_setting", { key, value }).catch(() => {});

  const saveCreds = async () => {
    try {
      await invoke("google_set_credentials", { clientId: clientId.trim(), clientSecret: clientSecret.trim() });
      setEditingCreds(false);
      await refresh();
      showToast("Credentials saved", "success");
    } catch (err) { showToast(`Failed: ${err}`, "error"); }
  };

  const signIn = async () => {
    setSigningIn(true);
    try {
      const email = await invoke<string>("google_sign_in");
      showToast(`Connected: ${email}`, "success");
      await refresh();
    } catch (err) {
      showToast(`Sign-in failed: ${err}`, "error");
    } finally {
      setSigningIn(false);
    }
  };

  const disconnect = async () => {
    try { await invoke("google_disconnect"); await refresh(); } catch { /* ignore */ }
  };

  const showCredForm = editingCreds || !status.has_credentials;

  return (
    <VStack gap={4} style={{ width: "100%" }}>
      <VStack gap={1}>
        <Text style={{ fontSize: 15, fontWeight: 700, color: "var(--color-text-primary)" }}>Google Calendar</Text>
        <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
          Connect your calendar to see upcoming meetings, get start reminders, and suggest attendee names as speaker labels.
        </Text>
      </VStack>

      {status.connected ? (
        <HStack style={{ justifyContent: "space-between", alignItems: "center", maxWidth: 520, padding: "10px 14px", borderRadius: 10, border: "1px solid rgba(16,185,129,0.4)", backgroundColor: "rgba(16,185,129,0.08)" }}>
          <VStack gap={0}>
            <Text style={{ fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" }}>Connected{status.email ? ` — ${status.email}` : ""}</Text>
            <Text style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>Calendar (read-only)</Text>
          </VStack>
          <Button variant="danger" size="sm" label="Disconnect" onClick={disconnect} />
        </HStack>
      ) : (
        <VStack gap={3} style={{ maxWidth: 520 }}>
          {showCredForm ? (
            <VStack gap={2} style={{ padding: 14, borderRadius: 10, border: "1px dashed var(--color-border-strong)", backgroundColor: "var(--color-background-surface)" }}>
              <Text style={{ fontSize: 12, color: "var(--color-text-secondary)" }}>
                Create a <strong>Desktop app</strong> OAuth client in Google Cloud Console (enable the Calendar API,
                add yourself as a test user), then paste its Client ID &amp; secret:
              </Text>
              <TextInput label="Client ID" value={clientId} onChange={setClientId} placeholder="xxxx.apps.googleusercontent.com" style={{ width: "100%" }} />
              <TextInput label="Client Secret" value={clientSecret} onChange={setClientSecret} placeholder="GOCSPX-…" style={{ width: "100%" }} />
              <HStack gap={2}>
                <Button variant="primary" size="sm" label="Save credentials" onClick={saveCreds} isDisabled={!clientId.trim() || !clientSecret.trim()} />
                {status.has_credentials && <Button variant="ghost" size="sm" label="Cancel" onClick={() => setEditingCreds(false)} />}
              </HStack>
            </VStack>
          ) : (
            <HStack gap={2} style={{ alignItems: "center" }}>
              <Button variant="primary" label={signingIn ? "Waiting for browser…" : "Sign in with Google"} onClick={signIn} isDisabled={signingIn} />
              <Button variant="ghost" size="sm" label="Edit credentials" onClick={() => setEditingCreds(true)} />
            </HStack>
          )}
        </VStack>
      )}

      {/* Reminder / auto-start settings */}
      <VStack gap={3} style={{ marginTop: 4 }}>
        <ToggleRow
          label="Meeting start reminders"
          description="Notify you when a calendar meeting is about to start, with a shortcut to record."
          checked={notify}
          onChange={() => { const v = !notify; setNotify(v); persist("meeting_notify_enabled", String(v)); }}
        />
        {notify && (
          <HStack gap={2} style={{ alignItems: "center", maxWidth: 520 }}>
            <Text style={labelStyle}>Notify</Text>
            <select value={minsBefore} onChange={(e) => { setMinsBefore(e.target.value); persist("meeting_notify_before_min", e.target.value); }} style={selectStyle}>
              {["0", "1", "2", "5", "10"].map((m) => <option key={m} value={m}>{m === "0" ? "at start" : `${m} min`}</option>)}
            </select>
            <Text style={labelStyle}>before the meeting</Text>
          </HStack>
        )}
        <ToggleRow
          label="Auto-start recording"
          description="Automatically start recording when a calendar meeting begins."
          checked={autoStart}
          onChange={() => { const v = !autoStart; setAutoStart(v); persist("meeting_autostart_enabled", String(v)); }}
        />
      </VStack>
    </VStack>
  );
}
