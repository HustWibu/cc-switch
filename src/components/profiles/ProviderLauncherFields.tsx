import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import {
  launchProfilesApi,
  type LaunchProfile,
} from "@/lib/api/launchProfiles";
import type { Provider } from "@/types";
import type { AppId } from "@/lib/api";

export interface LauncherDraft {
  alias: string;
  model: string;
  install: boolean;
}
export const emptyLauncherDraft: LauncherDraft = {
  alias: "",
  model: "",
  install: true,
};
export const supportsLauncher = (
  app: AppId,
): app is "claude" | "codex" | "opencode" =>
  ["claude", "codex", "opencode"].includes(app);

export function suggestLauncher(appId: AppId, name: string): string {
  const slug = name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${appId}-${slug || "work"}`.slice(0, 64).replace(/-+$/g, "");
}

export function ProviderLauncherFields({
  appId,
  providerName = "",
  value,
  onChange,
}: {
  appId: AppId;
  providerName?: string;
  value: LauncherDraft;
  onChange: (v: LauncherDraft) => void;
}) {
  const { t } = useTranslation();
  const previousSuggestion = useRef("");
  const manuallyEdited = useRef(false);
  useEffect(() => {
    if (!supportsLauncher(appId) || manuallyEdited.current) return;
    if (value.alias && value.alias !== previousSuggestion.current) return;
    const alias = suggestLauncher(appId, providerName);
    previousSuggestion.current = alias;
    if (value.alias !== alias) onChange({ ...value, alias });
  }, [appId, providerName, value, onChange]);
  if (!supportsLauncher(appId)) return null;
  return (
    <fieldset className="space-y-3 rounded-lg border p-4 my-4">
      <legend className="text-sm font-medium">
        {t("launchProfiles.providerSection")}
      </legend>
      <label className="block">
        {t("launchProfiles.optionalCommand")}
        <div className="flex items-center gap-2">
          <span className="shrink-0 font-mono text-sm">cc-switch</span>
          <Input
            value={value.alias}
            pattern="[A-Za-z][A-Za-z0-9_-]*"
            onChange={(e) => {
              manuallyEdited.current = true;
              onChange({ ...value, alias: e.target.value });
            }}
            placeholder={`${appId}-work`}
          />
        </div>
      </label>
      <label className="block">
        {t("launchProfiles.model")}
        <Input
          value={value.model}
          onChange={(e) => onChange({ ...value, model: e.target.value })}
        />
      </label>
      <label className="flex gap-2 items-center">
        <input
          type="checkbox"
          checked={value.install}
          onChange={(e) => onChange({ ...value, install: e.target.checked })}
        />
        {t("launchProfiles.installOnSave")}
      </label>
      <p className="text-sm text-muted-foreground">
        {t("launchProfiles.shellNotice")}
      </p>
    </fieldset>
  );
}

export async function saveProviderLauncher(
  appId: AppId,
  provider: Provider,
  draft: LauncherDraft,
) {
  if (!supportsLauncher(appId) || !draft.alias.trim()) return;
  const profiles = await launchProfilesApi.list();
  const existing = profiles.find(
    (p) =>
      (p.runtime ?? "claude") === appId &&
      p.providerId === provider.id &&
      p.launcher === draft.alias.trim(),
  );
  const profile: LaunchProfile = {
    id: "",
    name: provider.name,
    launcher: draft.alias.trim(),
    providerId: provider.id,
    runtime: appId,
    configDir: "",
    routingMode:
      appId === "claude"
        ? provider.category === "official"
          ? "official"
          : "proxy"
        : provider.category === "official" && appId === "codex"
          ? "official"
          : "direct",
    model: draft.model.trim() || null,
    revision: 0,
    createdAt: 0,
    updatedAt: 0,
    ...existing,
  };
  profile.model = draft.model.trim() || profile.model;
  const saved = await launchProfilesApi.save(profile);
  if (draft.install) await launchProfilesApi.install(saved.id);
  return saved;
}
