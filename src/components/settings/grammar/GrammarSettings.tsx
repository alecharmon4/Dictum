import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  commands,
  type GrammarApp,
  type GrammarDialect,
  type GrammarSettings as GrammarSettingsData,
} from "@/bindings";
import { Alert } from "../../ui/Alert";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

const DIALECTS: GrammarDialect[] = [
  "American",
  "British",
  "Canadian",
  "Australian",
  "Indian",
];

/** How often to refresh while waiting for Accessibility access to start the highlighter. */
const PERMISSION_POLL_MS = 2000;

type CommandResult<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: string };

const RemoveIcon: React.FC = () => (
  <svg
    className="w-3 h-3"
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
);

export const GrammarSettings: React.FC = () => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<GrammarSettingsData | null>(null);
  const [updating, setUpdating] = useState(false);
  const [newWord, setNewWord] = useState("");
  const [appQuery, setAppQuery] = useState("");
  const [appResults, setAppResults] = useState<GrammarApp[]>([]);

  const apply = useCallback(
    async (request: Promise<CommandResult<GrammarSettingsData>>) => {
      setUpdating(true);
      try {
        const result = await request;
        if (result.status === "ok") {
          setSettings(result.data);
        } else {
          toast.error(result.error);
        }
      } finally {
        setUpdating(false);
      }
    },
    [],
  );

  useEffect(() => {
    apply(commands.getGrammarSettings());
  }, [apply]);

  // The backend starts the highlighter once Accessibility is granted; reflect that here.
  const waitingForPermission =
    settings?.enabled && !settings.accessibility_granted;
  useEffect(() => {
    if (!waitingForPermission) return;
    const id = window.setInterval(async () => {
      const result = await commands.getGrammarSettings();
      if (result.status === "ok") setSettings(result.data);
    }, PERMISSION_POLL_MS);
    return () => window.clearInterval(id);
  }, [waitingForPermission]);

  useEffect(() => {
    const query = appQuery.trim();
    if (!query) {
      setAppResults([]);
      return;
    }
    let cancelled = false;
    commands.searchGrammarApps(query).then((result) => {
      if (cancelled) return;
      if (result.status === "ok") {
        const added = new Set(settings?.apps.map((app) => app.bundle_id));
        setAppResults(
          result.data.filter((app) => !added.has(app.bundle_id)).slice(0, 8),
        );
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appQuery, settings?.apps]);

  if (!settings) return null;

  if (!settings.supported) {
    return (
      <div className="max-w-3xl w-full mx-auto space-y-6">
        <Alert variant="info">{t("settings.grammar.unsupported")}</Alert>
      </div>
    );
  }

  const word = newWord.trim();
  const addWord = () => {
    if (!word) return;
    if (settings.dictionary.includes(word)) {
      toast.error(t("settings.grammar.dictionary.duplicate", { word }));
      return;
    }
    apply(commands.addGrammarWord(word));
    setNewWord("");
  };

  const addApp = (app: GrammarApp) => {
    apply(commands.setGrammarAppEnabled(app.bundle_id, true));
    setAppQuery("");
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.grammar.groups.checking")}>
        <ToggleSwitch
          checked={settings.enabled}
          onChange={(enabled) => apply(commands.setGrammarEnabled(enabled))}
          isUpdating={updating}
          label={t("settings.grammar.enabled.label")}
          description={t("settings.grammar.enabled.description")}
          descriptionMode="inline"
          grouped
        />
        {waitingForPermission && (
          <Alert variant="warning" contained>
            {t("settings.grammar.accessibilityNeeded")}
          </Alert>
        )}
        <SettingContainer
          title={t("settings.grammar.dialect.title")}
          description={t("settings.grammar.dialect.description")}
          grouped
        >
          <Dropdown
            options={DIALECTS.map((dialect) => ({
              value: dialect,
              label: t(`settings.grammar.dialect.options.${dialect}`),
            }))}
            selectedValue={settings.dialect}
            onSelect={(value) =>
              apply(commands.setGrammarDialect(value as GrammarDialect))
            }
            disabled={updating}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.grammar.dismissed.title")}
          description={t("settings.grammar.dismissed.description")}
          grouped
        >
          <Button
            variant="secondary"
            size="md"
            disabled={updating}
            onClick={async () => {
              await apply(commands.resetGrammarDismissed());
              toast.success(t("settings.grammar.dismissed.done"));
            }}
          >
            {t("settings.grammar.dismissed.reset")}
          </Button>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.grammar.groups.apps")}
        description={t("settings.grammar.apps.description")}
      >
        {settings.apps.map((app) => (
          <SettingContainer
            key={app.bundle_id}
            title={app.name}
            description={app.bundle_id}
            descriptionMode="inline"
            grouped
          >
            <div className="flex items-center gap-2">
              <label
                className={`flex items-center ${updating ? "cursor-not-allowed" : "cursor-pointer"}`}
              >
                <input
                  type="checkbox"
                  value=""
                  className="sr-only peer"
                  checked={app.enabled}
                  disabled={updating}
                  aria-label={t("settings.grammar.apps.toggle", {
                    app: app.name,
                  })}
                  onChange={(e) =>
                    apply(
                      commands.setGrammarAppEnabled(
                        app.bundle_id,
                        e.target.checked,
                      ),
                    )
                  }
                />
                <div className="relative w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-logo-primary rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-background-ui peer-disabled:opacity-50"></div>
              </label>
              <Button
                variant="ghost"
                size="sm"
                disabled={updating}
                aria-label={t("settings.grammar.apps.remove", {
                  app: app.name,
                })}
                onClick={() => apply(commands.removeGrammarApp(app.bundle_id))}
              >
                <RemoveIcon />
              </Button>
            </div>
          </SettingContainer>
        ))}
        <SettingContainer
          title={t("settings.grammar.apps.add")}
          description={t("settings.grammar.apps.addDescription")}
          grouped
        >
          <Input
            type="text"
            className="max-w-48"
            value={appQuery}
            onChange={(e) => setAppQuery(e.target.value)}
            placeholder={t("settings.grammar.apps.searchPlaceholder")}
            variant="compact"
          />
        </SettingContainer>
        {appResults.length > 0 && (
          <div className="px-4 p-2 flex flex-wrap gap-1">
            {appResults.map((app) => (
              <Button
                key={app.bundle_id}
                variant="secondary"
                size="sm"
                disabled={updating}
                onClick={() => addApp(app)}
                title={app.bundle_id}
              >
                + {app.name}
              </Button>
            ))}
          </div>
        )}
      </SettingsGroup>

      <SettingsGroup title={t("settings.grammar.groups.dictionary")}>
        <SettingContainer
          title={t("settings.grammar.dictionary.title")}
          description={t("settings.grammar.dictionary.description")}
          grouped
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addWord();
                }
              }}
              placeholder={t("settings.grammar.dictionary.placeholder")}
              variant="compact"
              disabled={updating}
            />
            <Button
              onClick={addWord}
              disabled={!word || updating}
              variant="primary"
              size="md"
            >
              {t("settings.grammar.dictionary.add")}
            </Button>
          </div>
        </SettingContainer>
        {settings.dictionary.length > 0 && (
          <div className="px-4 p-2 flex flex-wrap gap-1">
            {settings.dictionary.map((entry) => (
              <Button
                key={entry}
                onClick={() => apply(commands.removeGrammarWord(entry))}
                disabled={updating}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.grammar.dictionary.remove", {
                  word: entry,
                })}
              >
                <span>{entry}</span>
                <RemoveIcon />
              </Button>
            ))}
          </div>
        )}
      </SettingsGroup>
    </div>
  );
};
