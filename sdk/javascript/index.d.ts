export class OpenFangError extends Error {
  status: number;
  body: string;
  constructor(message: string, status: number, body: string);
}

export interface AgentCreateOpts {
  template?: string;
  name?: string;
  model?: string;
  [key: string]: unknown;
}

export interface MessageOpts {
  attachments?: string[];
  [key: string]: unknown;
}

export interface StreamEvent {
  type?: string;
  delta?: string;
  raw?: string;
  [key: string]: unknown;
}

export class OpenFang {
  baseUrl: string;
  agents: AgentResource;
  sessions: SessionResource;
  workflows: WorkflowResource;
  skills: SkillResource;
  channels: ChannelResource;
  tools: ToolResource;
  models: ModelResource;
  providers: ProviderResource;
  memory: MemoryResource;
  triggers: TriggerResource;
  schedules: ScheduleResource;

  constructor(baseUrl: string, opts?: { headers?: Record<string, string> });

  health(): Promise<unknown>;
  healthDetail(): Promise<unknown>;
  status(): Promise<unknown>;
  version(): Promise<unknown>;
  metrics(): Promise<string>;
  usage(): Promise<unknown>;
  config(): Promise<unknown>;
}

export class AgentResource {
  list(): Promise<unknown[]>;
  get(id: string): Promise<unknown>;
  create(
    opts: AgentCreateOpts
  ): Promise<{ id: string; [key: string]: unknown }>;
  delete(id: string): Promise<unknown>;
  stop(id: string): Promise<unknown>;
  clone(id: string): Promise<unknown>;
  update(id: string, data: Record<string, unknown>): Promise<unknown>;
  setMode(id: string, mode: string): Promise<unknown>;
  setModel(id: string, model: string): Promise<unknown>;
  message(id: string, text: string, opts?: MessageOpts): Promise<unknown>;
  stream(
    id: string,
    text: string,
    opts?: MessageOpts
  ): AsyncGenerator<StreamEvent>;
  session(id: string): Promise<unknown>;
  resetSession(id: string): Promise<unknown>;
  compactSession(id: string): Promise<unknown>;
  listSessions(id: string): Promise<unknown[]>;
  createSession(id: string, label?: string): Promise<unknown>;
  switchSession(id: string, sessionId: string): Promise<unknown>;
  getSkills(id: string): Promise<unknown>;
  setSkills(id: string, skills: unknown): Promise<unknown>;
  upload(id: string, file: Blob | File, filename: string): Promise<unknown>;
  setIdentity(id: string, identity: Record<string, unknown>): Promise<unknown>;
  patchConfig(id: string, config: Record<string, unknown>): Promise<unknown>;
}

export class SessionResource {
  list(): Promise<unknown[]>;
  delete(id: string): Promise<unknown>;
  setLabel(id: string, label: string): Promise<unknown>;
}

export class WorkflowResource {
  list(): Promise<unknown[]>;
  create(workflow: Record<string, unknown>): Promise<unknown>;
  run(id: string, input?: Record<string, unknown>): Promise<unknown>;
  runs(id: string): Promise<unknown[]>;
}

export class SkillResource {
  list(): Promise<unknown[]>;
  install(skill: Record<string, unknown>): Promise<unknown>;
  uninstall(skill: Record<string, unknown>): Promise<unknown>;
  search(query: string): Promise<unknown[]>;
}

export class ChannelResource {
  list(): Promise<unknown[]>;
  configure(name: string, config: Record<string, unknown>): Promise<unknown>;
  remove(name: string): Promise<unknown>;
  test(name: string): Promise<unknown>;
}

export class ToolResource {
  list(): Promise<unknown[]>;
}

export class ModelResource {
  list(): Promise<unknown[]>;
  get(id: string): Promise<unknown>;
  aliases(): Promise<unknown>;
  updateUnigptModels(models: Record<string, unknown>): Promise<unknown>;
}

export class ProviderResource {
  list(): Promise<unknown[]>;
  setKey(name: string, key: string): Promise<unknown>;
  deleteKey(name: string): Promise<unknown>;
  test(name: string): Promise<unknown>;
  setUrl(name: string, url: string): Promise<unknown>;
}

export class MemoryResource {
  getAll(agentId: string): Promise<Record<string, unknown>>;
  get(agentId: string, key: string): Promise<unknown>;
  set(agentId: string, key: string, value: unknown): Promise<unknown>;
  delete(agentId: string, key: string): Promise<unknown>;
}

export class TriggerResource {
  list(): Promise<unknown[]>;
  create(trigger: Record<string, unknown>): Promise<unknown>;
  update(id: string, trigger: Record<string, unknown>): Promise<unknown>;
  delete(id: string): Promise<unknown>;
}

export class ScheduleResource {
  list(): Promise<unknown[]>;
  create(schedule: Record<string, unknown>): Promise<unknown>;
  update(id: string, schedule: Record<string, unknown>): Promise<unknown>;
  delete(id: string): Promise<unknown>;
  run(id: string): Promise<unknown>;
}
