import { ProviderVerificationDialog } from "@/components/providers/ProviderVerificationDialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  launchProfilesApi,
  type LaunchProfile,
} from "@/lib/api/launchProfiles";
import { proxyApi } from "@/lib/api/proxy";
import { providersApi } from "@/lib/api/providers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";

const empty: LaunchProfile = {
  id: "",
  name: "",
  launcher: "",
  providerId: "",
  runtime: "claude",
  configDir: "",
  routingMode: "proxy",
  model: null,
  revision: 0,
  createdAt: 0,
  updatedAt: 0,
};

export function LaunchProfilesDialog({
  appId = "claude",
}: {
  appId?: "claude" | "codex" | "opencode";
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<LaunchProfile>({
    ...empty,
    runtime: appId,
    routingMode: appId === "claude" ? "proxy" : "direct",
  });
  const [error, setError] = useState("");
  const [connection, setConnection] = useState("");
  const client = useQueryClient();
  const runtime = draft.runtime ?? appId;
  const fresh = () => ({
    ...empty,
    runtime,
    routingMode: (runtime === "claude"
      ? "proxy"
      : "direct") as LaunchProfile["routingMode"],
  });
  const profiles = useQuery({
    queryKey: ["launchProfiles"],
    queryFn: launchProfilesApi.list,
    enabled: open,
  });
  const providers = useQuery({
    queryKey: ["launchProfileProviders", runtime],
    queryFn: () => providersApi.getAll(runtime),
    enabled: open,
  });
  const dispatcher = useQuery({
    queryKey: ["dispatcherStatus"],
    queryFn: launchProfilesApi.dispatcherStatus,
    enabled: open,
  });
  const defaultProfile = profiles.data?.find((p) => p.isDefault);
  const save = useMutation({
    mutationFn: launchProfilesApi.save,
    onSuccess: () => {
      setDraft(fresh());
      setError("");
      void client.invalidateQueries({ queryKey: ["launchProfiles"] });
    },
    onError: (e) => setError(String(e)),
  });
  const action = async (fn: () => Promise<void>) => {
    try {
      await fn();
      setError("");
      await client.invalidateQueries({ queryKey: ["launchProfiles"] });
      await client.invalidateQueries({ queryKey: ["launcherStatus"] });
      await client.invalidateQueries({ queryKey: ["dispatcherStatus"] });
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <>
      <Button variant="quiet" onClick={() => setOpen(true)}>
        {t("launchProfiles.title")}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{t("launchProfiles.title")}</DialogTitle>
            <DialogDescription>
              {t("launchProfiles.description")}
            </DialogDescription>
          </DialogHeader>
          {(error || profiles.error || providers.error) && (
            <p role="alert" className="text-destructive">
              {error || String(profiles.error || providers.error)}
            </p>
          )}
          <div className="rounded border p-3 space-y-2">
            <p>
              {t("launchProfiles.defaultProfile")}:{" "}
              {defaultProfile?.name ?? t("launchProfiles.noDefault")}
            </p>
            <p className="text-sm text-muted-foreground">
              {t("launchProfiles.defaultHelp")}
            </p>
            <code>cc-switch</code>
            <p className="text-xs">
              {dispatcher.data?.installed
                ? t("launchProfiles.installed")
                : t("launchProfiles.notInstalled")}
            </p>
            <Button
              variant="outline"
              onClick={() =>
                void action(async () => {
                  await launchProfilesApi.installDispatcher();
                  setConnection(t("launchProfiles.installedNotice"));
                })
              }
            >
              {t("launchProfiles.installDispatcher")}
            </Button>
            {dispatcher.error && <p role="alert">{String(dispatcher.error)}</p>}
          </div>
          <div className="space-y-3">
            {profiles.data
              ?.filter((p) => (p.runtime ?? "claude") === runtime)
              .map((p) => (
                <div key={p.id} className="rounded border p-3 space-y-2">
                  <p className="font-medium">{p.name}</p>
                  <code className="text-xs">cc-switch {p.launcher}</code>
                  {p.isDefault ? (
                    <p className="text-sm font-medium">
                      {t("launchProfiles.defaultBadge")}
                    </p>
                  ) : (
                    <Button
                      variant="outline"
                      onClick={() =>
                        void action(() => launchProfilesApi.setDefault(p.id))
                      }
                    >
                      {t("launchProfiles.setDefault")}
                    </Button>
                  )}
                  <LauncherActions profile={p} action={action} />
                  <ProviderVerificationDialog
                    appId={p.runtime ?? "claude"}
                    providerId={p.providerId}
                    profileId={p.id}
                  />
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      onClick={() =>
                        setDraft({ ...p, runtime: p.runtime ?? "claude" })
                      }
                    >
                      {t("launchProfiles.edit")}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() =>
                        setDraft({
                          ...p,
                          id: "",
                          name: t("launchProfiles.copyName", { name: p.name }),
                          launcher: `${p.launcher}-copy`,
                          configDir: "",
                          createdAt: 0,
                          updatedAt: 0,
                          revision: 0,
                        })
                      }
                    >
                      {t("launchProfiles.duplicate")}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() =>
                        void action(() => launchProfilesApi.launch(p.id))
                      }
                    >
                      {t("launchProfiles.launch")}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() =>
                        void action(() => launchProfilesApi.remove(p.id))
                      }
                    >
                      {t("launchProfiles.remove")}
                    </Button>
                  </div>
                </div>
              ))}
          </div>
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              void action(async () => {
                await proxyApi.startProxyServer();
                setConnection(t("launchProfiles.proxyStarted"));
              })
            }
          >
            {t("launchProfiles.startProxy")}
          </Button>
          {connection && (
            <p role="status" className="text-sm">
              {connection}
            </p>
          )}
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate(draft);
            }}
          >
            <label className="block">
              {t("launchProfiles.runtime")}
              <select
                className="w-full rounded border bg-background p-2"
                value={runtime}
                onChange={(e) =>
                  setDraft({
                    ...fresh(),
                    runtime: e.target.value as typeof runtime,
                    routingMode:
                      e.target.value === "claude" ? "proxy" : "direct",
                  })
                }
              >
                <option value="claude">Claude Code</option>
                <option value="codex">Codex</option>
                <option value="opencode">OpenCode</option>
              </select>
            </label>
            <label className="block">
              {t("launchProfiles.name")}
              <Input
                required
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </label>
            <label className="block">
              {t("launchProfiles.alias")}
              <div className="flex items-center gap-2">
                <span className="shrink-0 font-mono text-sm">cc-switch</span>
                <Input
                  required
                  pattern="[A-Za-z][A-Za-z0-9_-]*"
                  value={draft.launcher}
                  onChange={(e) =>
                    setDraft({ ...draft, launcher: e.target.value })
                  }
                />
              </div>
            </label>
            <label className="block">
              {t("launchProfiles.provider")}
              <select
                className="w-full rounded border bg-background p-2"
                required
                value={draft.providerId}
                onChange={(e) => {
                  const provider = providers.data?.[e.target.value];
                  setDraft({
                    ...draft,
                    providerId: e.target.value,
                    routingMode:
                      runtime === "claude"
                        ? provider?.category === "official"
                          ? "official"
                          : "proxy"
                        : runtime === "codex" &&
                            provider?.category === "official"
                          ? "official"
                          : "direct",
                  });
                }}
              >
                <option value="">{t("launchProfiles.selectProvider")}</option>
                {Object.values(providers.data ?? {}).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <p className="text-sm text-muted-foreground">
              {t(
                draft.routingMode === "proxy"
                  ? "launchProfiles.proxy"
                  : draft.routingMode === "official"
                    ? runtime === "codex"
                      ? "launchProfiles.codexOfficial"
                      : "launchProfiles.official"
                    : "launchProfiles.direct",
              )}
            </p>
            <label className="block">
              {t("launchProfiles.directory")}
              <Input
                value={draft.configDir}
                onChange={(e) =>
                  setDraft({ ...draft, configDir: e.target.value })
                }
              />
            </label>
            <label className="block">
              {t("launchProfiles.model")}
              <Input
                value={draft.model ?? ""}
                onChange={(e) =>
                  setDraft({ ...draft, model: e.target.value || null })
                }
              />
            </label>
            <div className="flex gap-2">
              <Button type="submit" disabled={save.isPending}>
                {t("launchProfiles.save")}
              </Button>
              <Button
                type="button"
                variant="outline"
                disabled={
                  !draft.name ||
                  !draft.launcher ||
                  !draft.providerId ||
                  save.isPending
                }
                onClick={() =>
                  void action(async () => {
                    const p = await launchProfilesApi.save({
                      ...draft,
                      runtime,
                    });
                    setDraft(p);
                    await launchProfilesApi.install(p.id);
                    setDraft(fresh());
                    setConnection(t("launchProfiles.installedNotice"));
                  })
                }
              >
                {t("launchProfiles.saveInstall")}
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => setDraft(fresh())}
              >
                {t("launchProfiles.new")}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}

function LauncherActions({
  profile,
  action,
}: {
  profile: LaunchProfile;
  action: (fn: () => Promise<void>) => Promise<void>;
}) {
  const { t } = useTranslation();
  const status = useQuery({
    queryKey: ["launcherStatus", profile.id, profile.launcher],
    queryFn: () => launchProfilesApi.status(profile.id),
  });
  return (
    <div className="space-y-2">
      <p className="text-xs text-muted-foreground">
        {status.data?.installed
          ? t("launchProfiles.installed")
          : t("launchProfiles.notInstalled")}
        {status.data?.path && ` · ${status.data.path}`}
      </p>
      {status.error && <p role="alert">{String(status.error)}</p>}
      <div className="flex gap-2">
        <Button
          variant="outline"
          onClick={() =>
            void action(async () => {
              await launchProfilesApi.install(profile.id);
            })
          }
        >
          {t("launchProfiles.install")}
        </Button>
        <Button
          variant="outline"
          onClick={() =>
            void action(async () => {
              await navigator.clipboard.writeText(
                status.data?.command ?? `cc-switch ${profile.launcher}`,
              );
            })
          }
        >
          {t("launchProfiles.copyCommand")}
        </Button>
      </div>
      {status.data?.installed && (
        <p className="text-xs text-muted-foreground">
          {t("launchProfiles.installedNotice")}
        </p>
      )}
    </div>
  );
}
