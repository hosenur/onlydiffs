import { contextBridge, ipcRenderer } from "electron";
import type {
  OnlyDiffsApi,
  ForgetProjectRequest,
  GetFileContentsRequest,
  GetHistoryRequest,
  OpenProjectRequest,
  SendClaudeMessageRequest,
  StageFileRequest,
} from "../shared/contract";
import { IpcChannel } from "../shared/contract";

/**
 * The whole surface the renderer gets. Nothing here forwards a channel name
 * from the caller, so the renderer can only reach the handlers listed below.
 */
const api: OnlyDiffsApi = {
  getDiff: () => ipcRenderer.invoke(IpcChannel.getDiff),
  getFileContents: (request: GetFileContentsRequest) =>
    ipcRenderer.invoke(IpcChannel.getFileContents, request),
  getHistory: (request: GetHistoryRequest) =>
    ipcRenderer.invoke(IpcChannel.getHistory, request),
  stageFile: (request: StageFileRequest) =>
    ipcRenderer.invoke(IpcChannel.stageFile, request),
  generateCommitMessage: () =>
    ipcRenderer.invoke(IpcChannel.generateCommitMessage),
  sendClaudeMessage: (request: SendClaudeMessageRequest) =>
    ipcRenderer.invoke(IpcChannel.sendClaudeMessage, request),
  writeClipboardText: (text: string) =>
    ipcRenderer.invoke(IpcChannel.writeClipboardText, text),
  listProjects: () => ipcRenderer.invoke(IpcChannel.listProjects),
  openProject: (request: OpenProjectRequest) =>
    ipcRenderer.invoke(IpcChannel.openProject, request),
  currentProject: () => ipcRenderer.invoke(IpcChannel.currentProject),
  forgetProject: (request: ForgetProjectRequest) =>
    ipcRenderer.invoke(IpcChannel.forgetProject, request),
};

contextBridge.exposeInMainWorld("onlydiffs", api);
