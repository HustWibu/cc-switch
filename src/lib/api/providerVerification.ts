import { invoke } from "@tauri-apps/api/core";
import type { AppId } from "@/lib/api";

export type VerificationState =
  | "not_tested"
  | "reachable"
  | "configured"
  | "signed_in"
  | "verified"
  | "not_required"
  | "missing"
  | "failed"
  | "rate_limited"
  | "unsupported";
export interface VerificationReport {
  endpoint: VerificationState;
  authentication: VerificationState;
  model: VerificationState;
  reason: string;
  httpStatus: number | null;
  testedAt: number;
  profileId: string | null;
  providerId: string;
}
export const providerVerificationApi = {
  verify: (options: {
    appType: AppId;
    providerId: string;
    profileId: string | null;
    model: string | null;
    generate: boolean;
  }) => invoke<VerificationReport>("verify_provider_configuration", options),
};
