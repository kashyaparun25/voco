import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VStack, HStack } from "@astryxdesign/core/Layout";
import { Text } from "@astryxdesign/core/Text";

/**
 * EngineSelector — a single, connected "Provider → Model" picker.
 *
 * Provider + model are chosen per task (dictation STT, meeting STT, summary
 * LLM) and stored in that task's own setting keys. Crucially the *model* is
 * per-task, not per-connection: this lets one provider (e.g. a single Groq
 * connection) serve Whisper for speech *and* an LLM for summaries without the
 * two overwriting each other.
 *
 * Model lists come from `list_provider_models(id)` (a plain `string[]`): for
 * embedded these are downloaded local models; for remote they are fetched live
 * from the provider's /models or /api/tags endpoint. Since those endpoints
 * don't tell us which model is STT vs LLM, we apply a name heuristic so each
 * picker only offers sensible choices.
 */

interface FullProvider {
  id: string;
  name: string;
  provider_type: string;
}

interface EmbeddedModel {
  id: string;
  name: string;
  category: string;
  is_downloaded: boolean;
}

interface EngineSelectorProps {
  /** Setting key storing the selected provider id. */
  providerKey: string;
  /** Setting key storing the selected model id for this task. */
  modelKey: string;
  /** Which kind of model this picker is for. */
  category: "stt" | "llm";
}

const selectStyle: React.CSSProperties = {
  padding: "10px 14px",
  borderRadius: "8px",
  backgroundColor: "var(--color-background-elevated)",
  color: "var(--color-text-primary)",
  border: "1px solid var(--color-border-strong)",
  width: "100%",
  fontSize: "14px",
  cursor: "pointer",
  outline: "none",
};

const inputStyle: React.CSSProperties = { ...selectStyle, cursor: "text" };

const labelStyle: React.CSSProperties = {
  fontSize: "13px",
  fontWeight: 600,
  color: "var(--color-text-secondary)",
};

const hintStyle: React.CSSProperties = { fontSize: "11px", color: "var(--color-text-secondary)" };

// Heuristics to split a provider's flat model list into STT vs LLM candidates,
// since /models returns everything the account can access.
const STT_RE = /whisper|distil|parakeet|\basr\b|speech|transcrib|voice|canary/i;
const NON_LLM_RE = /whisper|distil|parakeet|\basr\b|speech|transcrib|\btts\b|embed|rerank|guard|moderation|vision-ocr/i;

function filterByCategory(ids: string[], category: "stt" | "llm"): string[] {
  if (category === "stt") {
    const m = ids.filter((id) => STT_RE.test(id));
    return m.length ? m : ids;
  }
  const m = ids.filter((id) => !NON_LLM_RE.test(id));
  return m.length ? m : ids;
}

async function getSetting(key: string): Promise<string | null> {
  try {
    return await invoke<string | null>("get_setting", { key });
  } catch {
    return null;
  }
}

async function setSetting(key: string, value: string): Promise<void> {
  try {
    await invoke("set_setting", { key, value });
  } catch (err) {
    console.warn(`EngineSelector: failed to save "${key}":`, err);
  }
}

export default function EngineSelector({ providerKey, modelKey, category }: EngineSelectorProps) {
  const [providers, setProviders] = useState<FullProvider[]>([]);
  const [providerId, setProviderId] = useState<string>("embedded");
  const [model, setModel] = useState<string>("");
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState<boolean>(false);
  const [modelsFetched, setModelsFetched] = useState<boolean>(false);

  const isEmbedded = providerId === "embedded";
  // Embedded LLM has no per-task model control (the backend auto-discovers a
  // downloaded GGUF), so we surface an "Automatic" note instead of a dropdown.
  const embeddedLlmAuto = isEmbedded && category === "llm";

  const loadProviders = async (): Promise<FullProvider[]> => {
    let list: FullProvider[] = [];
    try {
      list = await invoke<FullProvider[]>("get_providers");
    } catch (err) {
      console.warn("EngineSelector: failed to load providers:", err);
    }
    if (!list.some((p) => p.id === "embedded" || p.provider_type === "embedded")) {
      list = [{ id: "embedded", name: "Embedded (Local)", provider_type: "embedded" }, ...list];
    }
    setProviders(list);
    return list;
  };

  // Fetch + filter the model options for a provider.
  const fetchOptions = async (pid: string): Promise<string[]> => {
    if (pid === "embedded") {
      try {
        const all = await invoke<EmbeddedModel[]>("list_models");
        return all.filter((m) => m.category === category && m.is_downloaded).map((m) => m.id);
      } catch {
        return [];
      }
    }
    try {
      const ids = await invoke<string[]>("list_provider_models", { id: pid });
      return filterByCategory(Array.isArray(ids) ? ids : [], category);
    } catch (err) {
      console.warn("EngineSelector: failed to load models:", err);
      return [];
    }
  };

  // Ensure the selected model is valid for the current option set; if not,
  // pick the first option and persist it so the backend never receives a model
  // that belongs to a different provider/role.
  const reconcile = async (opts: string[], current: string): Promise<string> => {
    if (opts.length === 0) return current; // free-text mode; leave as typed
    if (current && opts.includes(current)) return current;
    const next = opts[0];
    await setSetting(modelKey, next);
    return next;
  };

  const loadModelsFor = async (pid: string, current: string) => {
    setLoadingModels(true);
    setModelsFetched(false);
    const opts = await fetchOptions(pid);
    setModelOptions(opts);
    if (!(pid === "embedded" && category === "llm")) {
      const resolved = await reconcile(opts, current);
      setModel(resolved);
    }
    setModelsFetched(true);
    setLoadingModels(false);
  };

  // Initial load.
  useEffect(() => {
    (async () => {
      const list = await loadProviders();
      const savedProvider = (await getSetting(providerKey)) || "embedded";
      const pid = list.some((p) => p.id === savedProvider) ? savedProvider : "embedded";
      setProviderId(pid);
      const savedModel = (await getSetting(modelKey)) || "";
      setModel(savedModel);
      await loadModelsFor(pid, savedModel);
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleProviderChange = async (nextId: string) => {
    setProviderId(nextId);
    await setSetting(providerKey, nextId);
    // Start from the model saved for this task, then reconcile to the new
    // provider's available models.
    const savedModel = (await getSetting(modelKey)) || "";
    setModel(savedModel);
    await loadModelsFor(nextId, savedModel);
  };

  const handleModelChange = (value: string) => {
    setModel(value);
    void setSetting(modelKey, value);
  };

  return (
    <HStack gap={4} style={{ width: "100%", flexWrap: "wrap", alignItems: "flex-start" }}>
      {/* Provider */}
      <VStack gap={2} style={{ flex: 1, minWidth: 220 }}>
        <Text style={labelStyle}>Provider</Text>
        <select value={providerId} onChange={(e) => void handleProviderChange(e.target.value)} style={selectStyle}>
          {providers.map((p) => (
            <option key={p.id} value={p.id}>
              {p.id === "embedded" ? "Embedded (Local)" : `${p.name} (${p.provider_type})`}
            </option>
          ))}
        </select>
      </VStack>

      {/* Model */}
      <VStack gap={2} style={{ flex: 1, minWidth: 220 }}>
        <Text style={labelStyle}>Model</Text>

        {embeddedLlmAuto ? (
          <VStack gap={1} style={{ padding: "10px 0" }}>
            <Text style={{ fontSize: "13px", color: "var(--color-text-primary)" }}>Automatic</Text>
            <Text style={hintStyle}>The embedded LLM uses a downloaded local model automatically.</Text>
          </VStack>
        ) : loadingModels ? (
          <VStack gap={1} style={{ padding: "10px 0" }}>
            <Text style={hintStyle}>Loading models…</Text>
          </VStack>
        ) : modelOptions.length > 0 ? (
          <>
            <select value={model} onChange={(e) => handleModelChange(e.target.value)} style={selectStyle}>
              {model && !modelOptions.includes(model) && <option value={model}>{model} (current)</option>}
              {modelOptions.map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </select>
            <Text style={hintStyle}>
              {isEmbedded
                ? "Downloaded local models. Add more under AI Providers & Models."
                : category === "stt"
                ? "Speech-to-text models offered by this provider."
                : "Chat/LLM models offered by this provider."}
            </Text>
          </>
        ) : (
          <>
            <input
              type="text"
              value={model}
              placeholder={category === "stt" ? "e.g. whisper-large-v3-turbo" : "e.g. llama-3.1-8b-instant"}
              onChange={(e) => setModel(e.target.value)}
              onBlur={(e) => void setSetting(modelKey, e.target.value)}
              style={inputStyle}
            />
            <Text style={hintStyle}>
              {isEmbedded
                ? "No local models downloaded yet — add one under AI Providers & Models."
                : modelsFetched
                ? "Couldn't list models automatically — type the model name."
                : "Type the model name."}
            </Text>
          </>
        )}
      </VStack>
    </HStack>
  );
}
