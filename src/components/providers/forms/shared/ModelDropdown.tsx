import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import type { FetchedModel } from "@/lib/api/model-fetch";
import { modelsDevQueryOptions } from "@/lib/modelsDev";
import { resolveModelMetadata } from "@/lib/modelMetadata";
import type { ModelMetadataSourceProps } from "../FetchedModelPicker";

export function ModelDropdown({
  models,
  onSelect,
  metadataSource,
}: {
  models: FetchedModel[];
  onSelect: (id: string) => void;
  metadataSource?: ModelMetadataSourceProps;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const { data: modelsDev } = useQuery({
    ...modelsDevQueryOptions,
    enabled: open && !!metadataSource,
  });
  const presets = metadataSource?.presets?.();
  const isFree = (id: string) => {
    if (!metadataSource) return false;
    const cost = resolveModelMetadata(id, {
      baseUrl: metadataSource.baseUrl,
      presets,
      modelsDev,
    })?.cost;
    return cost?.input === 0 && cost.output === 0;
  };

  // Group models by vendor; missing ownedBy falls back to "Other"
  const grouped: Record<string, FetchedModel[]> = {};
  for (const model of models) {
    const vendor = model.ownedBy || "Other";
    if (!grouped[vendor]) grouped[vendor] = [];
    grouped[vendor].push(model);
  }
  const vendors = Object.keys(grouped).sort();

  return (
    <Popover modal open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          className="shrink-0"
          aria-label={t("providerForm.searchModelAriaLabel", {
            defaultValue: "Select model",
          })}
        >
          <ChevronDown className="h-4 w-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={4}
        collisionPadding={8}
        className="z-[200] w-72 p-0"
      >
        <Command
          label={t("providerForm.searchModelPlaceholder", {
            defaultValue: "Search models...",
          })}
        >
          <CommandInput
            placeholder={t("providerForm.searchModelPlaceholder", {
              defaultValue: "Search models...",
            })}
          />
          <CommandList className="max-h-64">
            <CommandEmpty>
              {t("providerForm.searchModelEmpty", {
                defaultValue: "No matching models.",
              })}
            </CommandEmpty>
            {vendors.map((vendor) => (
              <CommandGroup key={vendor} heading={vendor}>
                {grouped[vendor].map((m) => (
                  <CommandItem
                    key={m.id}
                    value={m.id}
                    // Expose the vendor name as a keyword so models can also be
                    // fuzzy-matched by vendor, not just by model id.
                    keywords={[m.ownedBy || "Other"]}
                    onSelect={() => {
                      onSelect(m.id);
                      setOpen(false);
                    }}
                  >
                    <span className="min-w-0 flex-1 truncate">{m.id}</span>
                    {isFree(m.id) && (
                      <span className="shrink-0 rounded-full bg-emerald-500/10 px-2 py-0.5 text-caption font-medium text-emerald-700 dark:text-emerald-400">
                        Free
                      </span>
                    )}
                  </CommandItem>
                ))}
              </CommandGroup>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
