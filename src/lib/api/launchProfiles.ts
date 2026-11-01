import { invoke } from "@tauri-apps/api/core";

export interface LaunchProfile {
  id: string;
  name: string;
  launcher: string;
  providerId: string;
  isDefault?: boolean;
  runtime?: "claude" | "codex" | "opencode";
  configDir: string;
  routingMode: "official" | "proxy" | "direct";
  model: string | null;
  revision: number;
  createdAt: number;
  updatedAt: number;
}

export const launchProfilesApi = {
  setDefault: (id: string) =>
    invoke<void>("set_default_launch_profile", { id }),
  installDispatcher: () => invoke<LauncherStatus>("install_profile_dispatcher"),
  dispatcherStatus: () => invoke<LauncherStatus>("profile_dispatcher_status"),
  list: () => invoke<LaunchProfile[]>("list_launch_profiles"),
  save: (profile: LaunchProfile) =>
    invoke<LaunchProfile>("save_launch_profile", { profile }),
  remove: (id: string) => invoke<void>("delete_launch_profile", { id }),
  install: (id: string) =>
    invoke<LauncherStatus>("install_profile_launcher", { id }),
  status: (id: string) =>
    invoke<LauncherStatus>("profile_launcher_status", { id }),
  launch: (id: string) => invoke<void>("launch_profile_terminal", { id }),
};

export interface LauncherStatus {
  path: string;
  installed: boolean;
  onPath: boolean;
  command?: string;
}
