import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { isPlainObject } from "@/lib/requestOverrides";
import type { FetchedModel } from "@/lib/api/model-fetch";
import {
  FetchedModelPicker,
  type FetchedModelSelection,
} from "./FetchedModelPicker";

export interface ClaudeModelPickerOption {
  model: string;
  label?: string;
  description?: string;
  [key: string]: unknown;
}

export interface ClaudeModelPickerConfig {
  options: ClaudeModelPickerOption[];
  replaceBuiltInOptions?: boolean;
  [key: string]: unknown;
}

export function ClaudeModelPickerField({
  value,
  onChange,
  models,
  onFetch,
  isFetching,
  baseUrl,
}: {
  value: unknown;
  onChange: (value: ClaudeModelPickerConfig | undefined) => void;
  models: FetchedModel[];
  onFetch: () => void;
  isFetching: boolean;
  baseUrl: string;
}) {
  const { t } = useTranslation();
  const config = isPlainObject(value) ? value : {};
  const options: ClaudeModelPickerOption[] = Array.isArray(config.options)
    ? config.options.filter(
        (row): row is ClaudeModelPickerOption =>
          isPlainObject(row) &&
          typeof row.model === "string" &&
          !!row.model.trim(),
      )
    : [];
  // New lists replace the role aliases; existing lists retain their chosen behavior.
  const replaceBuiltInOptions =
    config.replaceBuiltInOptions === undefined
      ? value === undefined
      : config.replaceBuiltInOptions === true;
  const update = (next: ClaudeModelPickerOption[]) => {
    onChange(
      next.length
        ? { ...config, replaceBuiltInOptions, options: next }
        : undefined,
    );
  };
  const apply = ({ add, remove }: FetchedModelSelection) => {
    const removed = new Set(remove);
    const kept = options.filter((row) => !removed.has(row.model));
    const ids = new Set(kept.map((row) => row.model));
    update([
      ...kept,
      ...add.filter((id) => !ids.has(id)).map((model) => ({ model })),
    ]);
  };

  return (
    <section className="space-y-3 rounded-lg border border-border p-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-medium">
          {t("providerForm.claudeModelPicker.title")}
        </h3>
        <FetchedModelPicker
          models={models}
          configuredModelIds={options.map((row) => row.model)}
          onApply={apply}
          onFetch={onFetch}
          isFetching={isFetching}
          metadataSource={{ baseUrl }}
        />
      </div>
      <p className="text-xs text-fg-2">
        {t("providerForm.claudeModelPicker.help")}
      </p>
      <label className="flex items-center gap-2 text-sm">
        <Checkbox
          checked={replaceBuiltInOptions}
          onCheckedChange={(checked) =>
            onChange({
              ...config,
              options,
              replaceBuiltInOptions: checked === true,
            })
          }
        />
        {t("providerForm.claudeModelPicker.replaceBuiltIn")}
      </label>
      {options.length === 0 ? (
        <p className="text-xs text-fg-2">
          {t("providerForm.claudeModelPicker.empty")}
        </p>
      ) : (
        options.map((row) => (
          <div key={row.model} className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm" title={row.model}>
              {row.model}
            </span>
            <Input
              className="min-w-0 flex-1"
              aria-label={t("providerForm.claudeModelPicker.labelFor", {
                model: row.model,
              })}
              placeholder={row.model}
              value={row.label ?? ""}
              onChange={(event) =>
                update(
                  options.map((item) =>
                    item.model === row.model
                      ? { ...item, label: event.target.value }
                      : item,
                  ),
                )
              }
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t("providerForm.claudeModelPicker.remove", {
                model: row.model,
              })}
              onClick={() =>
                update(options.filter((item) => item.model !== row.model))
              }
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        ))
      )}
    </section>
  );
}
