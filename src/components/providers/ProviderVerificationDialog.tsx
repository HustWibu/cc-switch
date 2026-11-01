import { ShieldCheck } from "lucide-react";
import { HoverTip } from "@/components/ui/hover-tip";
import { useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { AppId } from "@/lib/api";
import { launchProfilesApi } from "@/lib/api/launchProfiles";
import {
  providerVerificationApi,
  type VerificationReport,
} from "@/lib/api/providerVerification";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";

/** Tests persisted configuration only. Never runs inference automatically. */
export function ProviderVerificationDialog({
  appId,
  providerId,
  profileId,
  compact = false,
}: {
  appId: AppId;
  providerId: string;
  profileId?: string;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState(profileId ?? "");
  const [model, setModel] = useState("");
  const [consent, setConsent] = useState(false);
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<VerificationReport>();
  const [error, setError] = useState(false);
  const sequence = useRef(0);
  const profiles = useQuery({
    queryKey: ["launchProfiles"],
    queryFn: launchProfilesApi.list,
    enabled:
      open && !profileId && ["claude", "codex", "opencode"].includes(appId),
  });
  const choices =
    profiles.data?.filter(
      (p) => p.providerId === providerId && (p.runtime ?? "claude") === appId,
    ) ?? [];
  const reset = () => {
    sequence.current++;
    setResult(undefined);
    setError(false);
    setPending(false);
  };
  const run = async (generate: boolean) => {
    if (pending || (generate && !consent)) return;
    const request = ++sequence.current;
    setPending(true);
    setResult(undefined);
    setError(false);
    try {
      const report = await providerVerificationApi.verify({
        appType: appId,
        providerId,
        profileId: profileId ?? (selected || null),
        model: model.trim() || null,
        generate,
      });
      if (request === sequence.current) setResult(report);
    } catch {
      if (request === sequence.current) setError(true);
    } finally {
      if (request === sequence.current) setPending(false);
    }
  };
  return (
    <div onClick={(e) => e.stopPropagation()}>
      <HoverTip content={t("providerVerification.title")}>
        <Button
          type="button"
          variant={compact ? "quiet" : "outline"}
          size={compact ? "icon-compact" : undefined}
          aria-label={t("providerVerification.title")}
          onClick={() => {
            reset();
            setConsent(false);
            setOpen(true);
          }}
        >
          {compact ? (
            <ShieldCheck className="h-4 w-4" strokeWidth={1.5} />
          ) : (
            t("providerVerification.title")
          )}
        </Button>
      </HoverTip>
      <Dialog
        open={open}
        onOpenChange={(value) => {
          setOpen(value);
          if (!value) reset();
        }}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("providerVerification.title")}</DialogTitle>
            <DialogDescription>
              {t("providerVerification.description")}
            </DialogDescription>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            {t(
              profileId
                ? "providerVerification.privateProfile"
                : "providerVerification.savedOnly",
            )}
          </p>
          {!profileId && (
            <label className="space-y-1 text-sm">
              {t("providerVerification.configuration")}
              <select
                className="w-full rounded border bg-background p-2"
                disabled={pending}
                value={selected}
                onChange={(e) => {
                  setSelected(e.target.value);
                  reset();
                  setConsent(false);
                }}
              >
                <option value="">
                  {t("providerVerification.savedProvider")}
                </option>
                {choices.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.launcher}
                  </option>
                ))}
              </select>
            </label>
          )}
          {profiles.isError && (
            <p role="alert">{t("providerVerification.profilesUnavailable")}</p>
          )}
          <label className="space-y-1 text-sm">
            {t("providerVerification.modelInput")}
            <Input
              disabled={pending}
              value={model}
              maxLength={256}
              onChange={(e) => {
                setModel(e.target.value);
                reset();
              }}
            />
          </label>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={() => void run(false)}
            >
              {t("providerVerification.inspect")}
            </Button>
            <Button
              type="button"
              disabled={pending || !consent}
              onClick={() => void run(true)}
            >
              {t(
                pending
                  ? "providerVerification.checking"
                  : "providerVerification.generate",
              )}
            </Button>
          </div>
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={consent}
              disabled={pending}
              onChange={(e) => setConsent(e.target.checked)}
            />
            {t("providerVerification.consent")}
          </label>
          {error && (
            <p role="alert">{t("providerVerification.requestError")}</p>
          )}
          {result && (
            <div role="status" className="space-y-2 rounded border p-3 text-sm">
              <dl className="grid grid-cols-2 gap-2">
                {(["endpoint", "authentication", "model"] as const).map(
                  (key) => (
                    <div key={key} className="contents">
                      <dt>{t(`providerVerification.${key}`)}</dt>
                      <dd>{t(`providerVerification.states.${result[key]}`)}</dd>
                    </div>
                  ),
                )}
              </dl>
              <p>
                {t(`providerVerification.reasons.${result.reason}`, {
                  defaultValue: t("providerVerification.requestError"),
                })}
              </p>
              {result.httpStatus !== null && <p>HTTP {result.httpStatus}</p>}
              <p className="text-xs text-muted-foreground">
                {new Date(result.testedAt).toLocaleString()}
              </p>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
