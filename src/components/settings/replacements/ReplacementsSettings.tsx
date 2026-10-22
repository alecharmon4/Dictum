import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Replacement } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";

/** Wait this long after typing stops before previewing the sample text. */
const PREVIEW_DEBOUNCE_MS = 250;

const splitList = (value: string) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const joinList = (values: string[] | undefined) => (values ?? []).join(", ");

const newRule = (): Replacement => ({
  id: crypto.randomUUID(),
  variants: [],
  replacement: "",
  context_keywords: [],
  case_sensitive: false,
  enabled: true,
});

interface RuleRowProps {
  rule: Replacement;
  disabled: boolean;
  onChange: (rule: Replacement) => void;
  onRemove: () => void;
}

/** One rule. Text fields keep a local draft and save on blur so typing stays smooth. */
const RuleRow: React.FC<RuleRowProps> = ({
  rule,
  disabled,
  onChange,
  onRemove,
}) => {
  const { t } = useTranslation();
  const [variants, setVariants] = useState(joinList(rule.variants));
  const [replacement, setReplacement] = useState(rule.replacement);
  const [keywords, setKeywords] = useState(joinList(rule.context_keywords));

  const commit = () => {
    const next = {
      ...rule,
      variants: splitList(variants),
      replacement: replacement.trim(),
      context_keywords: splitList(keywords),
    };
    if (JSON.stringify(next) !== JSON.stringify(rule)) onChange(next);
  };

  const enabled = rule.enabled ?? true;

  return (
    <div className={`px-4 py-3 space-y-2 ${enabled ? "" : "opacity-60"}`}>
      <div className="flex items-end gap-2">
        <label className="flex-1 space-y-1">
          <span className="text-xs text-mid-gray">
            {t("settings.replacements.heardAs")}
          </span>
          <Input
            type="text"
            className="w-full"
            variant="compact"
            value={variants}
            onChange={(e) => setVariants(e.target.value)}
            onBlur={commit}
            placeholder={t("settings.replacements.heardAsPlaceholder")}
            disabled={disabled}
          />
        </label>
        <span className="pb-1.5 text-mid-gray" aria-hidden>
          →
        </span>
        <label className="w-44 space-y-1">
          <span className="text-xs text-mid-gray">
            {t("settings.replacements.replaceWith")}
          </span>
          <Input
            type="text"
            className="w-full"
            variant="compact"
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            onBlur={commit}
            placeholder={t("settings.replacements.replaceWithPlaceholder")}
            disabled={disabled}
          />
        </label>
        <Button
          variant="danger-ghost"
          size="sm"
          onClick={onRemove}
          disabled={disabled}
          aria-label={t("settings.replacements.remove")}
          title={t("settings.replacements.remove")}
        >
          <svg
            className="w-4 h-4"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </Button>
      </div>
      <div className="flex items-center gap-4">
        <Input
          type="text"
          className="flex-1"
          variant="compact"
          value={keywords}
          onChange={(e) => setKeywords(e.target.value)}
          onBlur={commit}
          placeholder={t("settings.replacements.onlyIfPlaceholder")}
          aria-label={t("settings.replacements.onlyIf")}
          title={t("settings.replacements.onlyIfDescription")}
          disabled={disabled}
        />
        <label className="flex items-center gap-1.5 text-xs cursor-pointer">
          <input
            type="checkbox"
            checked={rule.case_sensitive ?? false}
            onChange={(e) =>
              onChange({ ...rule, case_sensitive: e.target.checked })
            }
            disabled={disabled}
          />
          {t("settings.replacements.matchCase")}
        </label>
        <label className="flex items-center gap-1.5 text-xs cursor-pointer">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => onChange({ ...rule, enabled: e.target.checked })}
            disabled={disabled}
          />
          {t("settings.replacements.enabled")}
        </label>
      </div>
    </div>
  );
};

export const ReplacementsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const rules = getSetting("replacements") ?? [];
  const updating = isUpdating("replacements");
  const [sample, setSample] = useState("");
  const [preview, setPreview] = useState("");

  const save = (next: Replacement[]) => updateSetting("replacements", next);

  useEffect(() => {
    if (!sample.trim()) {
      setPreview("");
      return;
    }
    let cancelled = false;
    const id = window.setTimeout(async () => {
      const result = await commands.previewReplacements(sample, rules);
      if (!cancelled) setPreview(result);
    }, PREVIEW_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [sample, rules]);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("settings.replacements.title")}
        description={t("settings.replacements.description")}
      >
        {rules.length === 0 && (
          <p className="px-4 py-3 text-sm text-mid-gray">
            {t("settings.replacements.empty")}
          </p>
        )}
        {rules.map((rule) => (
          <RuleRow
            key={rule.id}
            rule={rule}
            disabled={updating}
            onChange={(changed) =>
              save(rules.map((r) => (r.id === changed.id ? changed : r)))
            }
            onRemove={() => save(rules.filter((r) => r.id !== rule.id))}
          />
        ))}
        <div className="px-4 py-3">
          <Button
            variant="secondary"
            size="md"
            onClick={() => save([...rules, newRule()])}
            disabled={updating}
          >
            + {t("settings.replacements.add")}
          </Button>
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.replacements.tryIt.title")}
        description={t("settings.replacements.tryIt.description")}
      >
        <div className="px-4 py-3 space-y-2">
          <Textarea
            className="w-full"
            variant="compact"
            value={sample}
            onChange={(e) => setSample(e.target.value)}
            placeholder={t("settings.replacements.tryIt.placeholder")}
          />
          {preview && (
            <div className="text-sm">
              <span className="text-xs text-mid-gray block mb-1">
                {t("settings.replacements.tryIt.result")}
              </span>
              {preview}
            </div>
          )}
        </div>
      </SettingsGroup>
    </div>
  );
};
