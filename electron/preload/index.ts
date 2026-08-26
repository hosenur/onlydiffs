import { contextBridge, ipcRenderer } from "electron";
import type {
  OnlyDiffsApi,
  GetFileContentsRequest,
  GetHistoryRequest,
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
};

contextBridge.exposeInMainWorld("onlydiffs", api);
