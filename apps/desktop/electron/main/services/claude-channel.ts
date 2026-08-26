import { homedir } from "node:os";
import * as path from "node:path";
import { FileSystem, HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect, Option, Schema } from "effect";
import type { ClaudeChannelStatus } from "../../shared/contract";
import { ClaudeChannelError } from "../errors";
import { Workspace } from "./workspace";

const MAX_MESSAGE_BYTES = 64 * 1024;
const SEND_TIMEOUT = "10 seconds";
const REGISTRATIONS_DIR = ".onlydiffs/claude-channels";
const MESSAGE_ID_PATTERN = /^[A-Za-z0-9_-]+$/;

const Registration = Schema.Struct({
  schemaVersion: Schema.Number,
  pid: Schema.Number,
  cwd: Schema.String,
  port: Schema.Number,
  token: Schema.String,
  startedAt: Schema.Number,
});
type Registration = Schema.Schema.Type<typeof Registration>;

const MessageAccepted = Schema.Struct({ messageId: Schema.String });

const decodeRegistration = Schema.decodeUnknown(Schema.parseJson(Registration));
const decodeAccepted = Schema.decodeUnknown(Schema.parseJson(MessageAccepted));

const NO_CHANNEL_MESSAGE =
  "No OnlyDiffs Claude channel is running. Restart Claude Code with the OnlyDiffs channel enabled.";

export class ClaudeChannel extends Effect.Service<ClaudeChannel>()(
  "onlydiffs/ClaudeChannel",
  {
    effect: Effect.gen(function* () {
      const workspace = yield* Workspace;
      const fs = yield* FileSystem.FileSystem;
      const client = yield* HttpClient.HttpClient;

      const fail = (message: string) => new ClaudeChannelError({ message });

      /** Live channels for this repository, newest first. */
      const registrations: Effect.Effect<
        Registration[],
        ClaudeChannelError
      > = Effect.gen(function* () {
        const repoPath = yield* workspace.currentPath.pipe(
          Effect.mapError((error) => fail(error.message)),
        );
        const directory = path.join(homedir(), REGISTRATIONS_DIR);

        const entries = yield* fs.readDirectory(directory).pipe(
          Effect.catchAll((error) =>
            error._tag === "SystemError" && error.reason === "NotFound"
              ? fail(NO_CHANNEL_MESSAGE)
              : fail(
                  `failed to read Claude channel registrations: ${error.message}`,
                ),
          ),
        );

        const found = yield* Effect.all(
          entries
            .filter((entry) => entry.endsWith(".json"))
            .map((entry) =>
              fs.readFileString(path.join(directory, entry)).pipe(
                Effect.flatMap(decodeRegistration),
                Effect.map(Option.some<Registration>),
                // A half-written or stale file is not an error worth showing;
                // the next candidate may well be live.
                Effect.orElseSucceed(() => Option.none<Registration>()),
              ),
            ),
          { concurrency: 8 },
        );

        const live = found
          .filter(Option.isSome)
          .map((entry) => entry.value)
          .filter(
            (entry) =>
              entry.schemaVersion === 1 &&
              entry.pid > 0 &&
              entry.port > 0 &&
              entry.token.length > 0 &&
              path.resolve(entry.cwd) === repoPath,
          )
          .sort((a, b) => b.startedAt - a.startedAt);

        if (live.length === 0) {
          return yield* fail(
            "No OnlyDiffs Claude channel is running for this repository. Restart Claude Code in this repository with the OnlyDiffs channel enabled.",
          );
        }
        return live;
      });

      const describeResponse = (status: number, body: string) => {
        const detail = body.trim();
        return detail.length === 0 ? `${status}` : `${status}: ${detail}`;
      };

      const readResponse = (request: HttpClientRequest.HttpClientRequest) =>
        client.execute(request).pipe(
          Effect.flatMap((response) =>
            response.text.pipe(
              Effect.map((body) => ({ status: response.status, body })),
            ),
          ),
          Effect.scoped,
        );

      /**
       * Sends a user-authored message into the live Claude Code session for
       * this repository. One direction only: the message is handed over and
       * that is the end of it — whatever Claude does next happens in Claude
       * Code, not here. A per-process bearer token protects the loopback
       * bridge.
       *
       * Resolves with the channel's message id, which is only useful for
       * correlating logs.
       */
      const send = (
        rawMessage: string,
      ): Effect.Effect<string, ClaudeChannelError> =>
        Effect.gen(function* () {
          const message = rawMessage.trim();
          if (message.length === 0) {
            return yield* fail("Message cannot be empty.");
          }
          if (Buffer.byteLength(message, "utf8") > MAX_MESSAGE_BYTES) {
            return yield* fail(
              `Message is too large (maximum ${MAX_MESSAGE_BYTES} bytes).`,
            );
          }

          const channels = yield* registrations;
          let lastError: string | null = null;

          for (const channel of channels) {
            const baseUrl = `http://127.0.0.1:${channel.port}`;

            const accepted = yield* readResponse(
              HttpClientRequest.post(`${baseUrl}/messages`).pipe(
                HttpClientRequest.bearerToken(channel.token),
                HttpClientRequest.bodyText(message),
              ),
            ).pipe(
              Effect.timeoutFail({
                duration: SEND_TIMEOUT,
                onTimeout: () =>
                  fail(`channel did not accept the message within ${SEND_TIMEOUT}`),
              }),
              Effect.map(Option.some),
              // A dead channel is expected — try the next one before giving up.
              Effect.catchAll((error) =>
                Effect.sync(() => {
                  lastError =
                    "_tag" in error && error._tag === "ClaudeChannelError"
                      ? error.message
                      : error.message;
                  return Option.none<{ status: number; body: string }>();
                }),
              ),
            );

            if (Option.isNone(accepted)) continue;
            const { status, body } = accepted.value;
            if (status < 200 || status >= 300) {
              lastError = `channel returned ${describeResponse(status, body)}`;
              continue;
            }

            const messageId = yield* decodeAccepted(body).pipe(
              Effect.map((accepted) => accepted.messageId),
              Effect.mapError((error) =>
                fail(`failed to decode the channel message ID: ${error.message}`),
              ),
            );
            if (!MESSAGE_ID_PATTERN.test(messageId)) {
              return yield* fail(
                "The Claude channel returned an invalid message ID.",
              );
            }

            return messageId;
          }

          return yield* fail(
            `Could not reach a OnlyDiffs Claude channel for this repository. Restart Claude Code with the OnlyDiffs channel enabled.${
              lastError === null ? "" : ` Last error: ${lastError}`
            }`,
          );
        });

      /**
       * Whether a Claude Code session is listening for this repository.
       *
       * Reports rather than throws: "no channel" is an ordinary state for a
       * status indicator, not a failure, and it is polled often enough that
       * turning it into an error channel would just mean catching it again.
       */
      const status: Effect.Effect<ClaudeChannelStatus> = registrations.pipe(
        Effect.map((live) => ({ connected: live.length > 0, sessions: live.length })),
        Effect.orElseSucceed(() => ({ connected: false, sessions: 0 })),
      );

      return { send, status } as const;
    }),
    dependencies: [Workspace.Default],
  },
) {}
