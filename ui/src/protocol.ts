// Backwards-compatible import path for Ink components. The definitions live
// in the shared package so Electron and Ink cannot silently drift apart.
export {
  PROTOCOL_VERSION,
  assertProtocolVersion,
  encodeClientMessage,
  parseServerEvent,
} from "../../shared/protocol";
export type {
  AgentEvent,
  AppConfig,
  ClientMessage,
  ModelConfig,
  ProviderCategory,
  ProviderProtocol,
  ServerEvent,
  SessionInfo,
  SessionMessage,
  SessionView,
  SupportedModel,
  SupportedProvider,
} from "../../shared/protocol";
