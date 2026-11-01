import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "../utils/testQueryClient";
import { ProviderVerificationDialog } from "@/components/providers/ProviderVerificationDialog";
import { providerVerificationApi } from "@/lib/api/providerVerification";
import { launchProfilesApi } from "@/lib/api/launchProfiles";
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/lib/api/providerVerification", () => ({
  providerVerificationApi: { verify: vi.fn() },
}));
vi.mock("@/lib/api/launchProfiles", () => ({
  launchProfilesApi: { list: vi.fn() },
}));
const report = {
  endpoint: "reachable" as const,
  authentication: "failed" as const,
  model: "failed" as const,
  reason: "unauthorized",
  httpStatus: 401,
  testedAt: 1,
  profileId: "private",
  providerId: "provider",
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(launchProfilesApi.list).mockResolvedValue([]);
  vi.mocked(providerVerificationApi.verify).mockResolvedValue(report);
});
function open(
  appId: "claude" | "opencode" | "gemini" = "claude",
  profileId?: string,
) {
  renderWithQueryClient(
    <ProviderVerificationDialog
      appId={appId}
      providerId="provider"
      profileId={profileId}
    />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "providerVerification.title" }),
  );
}
describe("provider verification", () => {
  it("does not call a provider on open and requires consent before generation", async () => {
    open();
    expect(providerVerificationApi.verify).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "providerVerification.generate" }),
    ).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(
      screen.getByRole("button", { name: "providerVerification.generate" }),
    );
    await waitFor(() =>
      expect(providerVerificationApi.verify).toHaveBeenCalledWith({
        appType: "claude",
        providerId: "provider",
        profileId: null,
        model: null,
        generate: true,
      }),
    );
  });
  it("inspects the private profile without generating or using the global account", async () => {
    open("opencode", "private");
    fireEvent.click(
      screen.getByRole("button", { name: "providerVerification.inspect" }),
    );
    await waitFor(() =>
      expect(providerVerificationApi.verify).toHaveBeenCalledWith(
        expect.objectContaining({
          appType: "opencode",
          profileId: "private",
          generate: false,
        }),
      ),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "providerVerification.states.reachable",
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "providerVerification.states.failed",
    );
    expect(screen.getByRole("status")).not.toHaveTextContent(
      "providerVerification.states.verified",
    );
  });
  it("offers the same diagnostic on providers outside OpenCode and the profile runtimes", async () => {
    open("gemini");
    fireEvent.click(
      screen.getByRole("button", { name: "providerVerification.inspect" }),
    );
    await waitFor(() =>
      expect(providerVerificationApi.verify).toHaveBeenCalledWith(
        expect.objectContaining({ appType: "gemini", generate: false }),
      ),
    );
    expect(launchProfilesApi.list).not.toHaveBeenCalled();
  });
  it("clears previous evidence when the requested model changes", async () => {
    open();
    fireEvent.click(
      screen.getByRole("button", { name: "providerVerification.inspect" }),
    );
    await screen.findByRole("status");
    fireEvent.change(screen.getByLabelText("providerVerification.modelInput"), {
      target: { value: "other-model" },
    });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
  it("selects a matching saved profile by immutable ID", async () => {
    vi.mocked(launchProfilesApi.list).mockResolvedValue([
      {
        id: "private",
        name: "Private",
        providerId: "provider",
        launcher: "gpt",
        runtime: "claude",
        configDir: "/private",
        routingMode: "proxy",
        model: null,
        revision: 1,
        createdAt: 1,
        updatedAt: 1,
      },
    ]);
    open();
    await screen.findByRole("option", { name: "Private · gpt" });
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "private" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "providerVerification.inspect" }),
    );
    await waitFor(() =>
      expect(providerVerificationApi.verify).toHaveBeenCalledWith(
        expect.objectContaining({ profileId: "private" }),
      ),
    );
  });
});
