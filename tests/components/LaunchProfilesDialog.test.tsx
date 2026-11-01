import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "../utils/testQueryClient";
import { LaunchProfilesDialog } from "@/components/profiles/LaunchProfilesDialog";
import { launchProfilesApi } from "@/lib/api/launchProfiles";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("@/lib/api/launchProfiles", () => ({ launchProfilesApi: { list: vi.fn(), save: vi.fn(), remove: vi.fn(), launch: vi.fn(), install: vi.fn(), setDefault: vi.fn(), installDispatcher: vi.fn(), dispatcherStatus: vi.fn().mockResolvedValue({installed:false,path:"/mock/bin/cc-switch",onPath:false}), status: vi.fn().mockResolvedValue({installed:false,path:"/mock/bin/claude-one",onPath:false}) } }));
vi.mock("@/lib/api/providers", () => ({ providersApi: { getAll: vi.fn().mockResolvedValue({ official: { id: "official", name: "Anthropic", category: "official" } }) } }));
const profile = { id: "one", name: "One", launcher: "claude-one", providerId: "official", configDir: "/private/profile-one", routingMode: "official" as const, model: null, revision: 1, createdAt: 1, updatedAt: 1 };

async function open() {
  renderWithQueryClient(<LaunchProfilesDialog />);
  fireEvent.click(screen.getByRole("button", { name: "launchProfiles.title" }));
  await screen.findByText("One");
}

describe("Claude launch profiles", () => {
  beforeEach(() => { vi.clearAllMocks(); vi.mocked(launchProfilesApi.list).mockResolvedValue([profile]); });
  it("sets the default by immutable ID without launching or changing provider settings",async()=>{
    vi.mocked(launchProfilesApi.setDefault).mockResolvedValue(undefined);
    await open();
    fireEvent.click(screen.getByRole("button",{name:"launchProfiles.setDefault"}));
    await waitFor(()=>expect(launchProfilesApi.setDefault).toHaveBeenCalledWith("one"));
    expect(launchProfilesApi.launch).not.toHaveBeenCalled();
    expect(launchProfilesApi.save).not.toHaveBeenCalled();
  });
  it("duplicates metadata with a fresh directory and identity", async () => {
    vi.mocked(launchProfilesApi.save).mockResolvedValue({ ...profile, id: "two" });
    await open();
    fireEvent.click(screen.getByRole("button", { name: "launchProfiles.duplicate" }));
    expect(screen.getByLabelText("launchProfiles.directory")).toHaveValue("");
    fireEvent.submit(screen.getByRole("button", { name: "launchProfiles.save" }).closest("form")!);
    await waitFor(() => expect(vi.mocked(launchProfilesApi.save).mock.calls[0]?.[0]).toEqual(expect.objectContaining({ id: "", configDir: "", launcher: "claude-one-copy", revision: 0 })));
  });
  it("shows explicit launch errors", async () => {
    vi.mocked(launchProfilesApi.launch).mockRejectedValue(new Error("Provider missing"));
    await open();
    fireEvent.click(screen.getByRole("button", { name: "launchProfiles.launch" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Provider missing");
  });
  it("removes the selected profile without deleting a directory through IPC", async () => {
    vi.mocked(launchProfilesApi.remove).mockResolvedValue(undefined);
    await open();
    fireEvent.click(screen.getByRole("button", { name: "launchProfiles.remove" }));
    await waitFor(() => expect(launchProfilesApi.remove).toHaveBeenCalledWith("one"));
    expect(launchProfilesApi.save).not.toHaveBeenCalled();
  });
});
