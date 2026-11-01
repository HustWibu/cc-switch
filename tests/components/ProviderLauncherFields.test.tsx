import { useState } from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  ProviderLauncherFields,
  emptyLauncherDraft,
} from "@/components/profiles/ProviderLauncherFields";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { saveProviderLauncher } from "@/components/profiles/ProviderLauncherFields";
import { launchProfilesApi } from "@/lib/api/launchProfiles";
import type { Provider } from "@/types";
vi.mock("@/lib/api/launchProfiles", () => ({
  launchProfilesApi: { list: vi.fn(), save: vi.fn(), install: vi.fn() },
}));
const provider: Provider = {
  id: "stable-id",
  name: "Work",
  category: "official",
  settingsConfig: {},
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(launchProfilesApi.list).mockResolvedValue([]);
  vi.mocked(launchProfilesApi.save).mockImplementation(async (p) => ({
    ...p,
    id: p.id || "new-profile",
  }));
  vi.mocked(launchProfilesApi.install).mockResolvedValue({
    installed: true,
    onPath: false,
    path: "/mock/bin/codex-work",
  });
});
describe("provider form launch commands", () => {
  it("saves the exact provider ID and runtime then installs the returned profile", async () => {
    await saveProviderLauncher("codex", provider, {
      alias: "codex-work",
      model: "model-one",
      install: true,
    });
    expect(launchProfilesApi.save).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: "stable-id",
        runtime: "codex",
        routingMode: "official",
        launcher: "codex-work",
      }),
    );
    expect(launchProfilesApi.install).toHaveBeenCalledWith("new-profile");
  });
  it("does not change profiles when no command was requested", async () => {
    await saveProviderLauncher("claude", provider, {
      alias: "",
      model: "",
      install: true,
    });
    expect(launchProfilesApi.save).not.toHaveBeenCalled();
    expect(launchProfilesApi.install).not.toHaveBeenCalled();
  });
  it("preserves a saved profile for retry when installation fails", async () => {
    vi.mocked(launchProfilesApi.install).mockRejectedValue(
      new Error("command already exists"),
    );
    await expect(
      saveProviderLauncher("opencode", provider, {
        alias: "opencode-work",
        model: "model",
        install: true,
      }),
    ).rejects.toThrow("command already exists");
    expect(launchProfilesApi.save).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: "stable-id",
        runtime: "opencode",
        routingMode: "direct",
      }),
    );
  });
});

function LauncherForm({ name }: { name: string }) {
  const [value, onChange] = useState(emptyLauncherDraft);
  return (
    <ProviderLauncherFields
      appId="claude"
      providerName={name}
      value={value}
      onChange={onChange}
    />
  );
}

describe("suggested launch command", () => {
  it("follows the provider name until the command is edited", () => {
    const view = render(<LauncherForm name="OpenCode Go" />);
    const input = screen.getByPlaceholderText("claude-work");
    expect(input).toHaveValue("claude-opencode-go");
    view.rerender(<LauncherForm name="My Provider" />);
    expect(input).toHaveValue("claude-my-provider");
    fireEvent.change(input, { target: { value: "my-custom-command" } });
    view.rerender(<LauncherForm name="Another Provider" />);
    expect(input).toHaveValue("my-custom-command");
  });
  it("lets the user clear the command to skip installation", () => {
    const view = render(<LauncherForm name="OpenCode Go" />);
    const input = screen.getByPlaceholderText("claude-work");
    fireEvent.change(input, { target: { value: "" } });
    view.rerender(<LauncherForm name="Another Provider" />);
    expect(input).toHaveValue("");
  });
});
