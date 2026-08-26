import { homedir } from "node:os";
import * as path from "node:path";
import { FileSystem, HttpClient, HttpClientRequest } from "@effect/platform";
import { Effect, Option, Schedule, Schema } from "effect";
import { ClaudeChannelError } from "../errors";
import { RepoConfig } from "./repo-config";

const MAX_MESSAGE_BYTES = 64 * 1024;
const SEND_TIMEOUT = "10 seconds";
/** Claude may work for a long time before the reply tool fires. */
const REPLY_TIMEOUT = "610 seconds";
/** How long to wait before re-opening a reply poll the server has dropped. */
const REPLY_POLL_INTERVAL = "500 millis";
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
const MessageReply = Schema.Struct({ text: Schema.String });

const decodeRegistration = Schema.decodeUnknown(Schema.parseJson(Registration));
const decodeAccepted = Schema.decodeUnknown(Schema.parseJson(MessageAccepted));
const decodeReply = Schema.decodeUnknown(Schema.parseJson(MessageReply));

const NO_CHANNEL_MESSAGE =
  "No OnlyDiffs Claude channel is running. Restart Claude Code with the OnlyDiffs channel enabled.";

export class ClaudeChannel extends Effect.Service<ClaudeChannel>()(
  "onlydiffs/ClaudeChannel",
  {
    effect: Effect.gen(function* () {
      const { repoPath } = yield* RepoConfig;
      const fs = yield* FileSystem.FileSystem;
      const client = yield* HttpClient.HttpClient;

      const fail = (message: string) => new ClaudeChannelError({ message });

      /** Live channels for this repository, newest first. */
      const registrations: Effect.Effect<
        Registration[],
        ClaudeChannelError
      > = Effect.gen(function* () {
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
       * Sends a user-authored message to the newest live OnlyDiffs channel and
       * waits for Claude to call the channel's reply tool with one complete
       * response. A per-process bearer token protects both sides of the
       * loopback HTTP bridge.
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

            // Past this point the message is in Claude's hands, so a failure is
            // reported rather than retried against another channel.
            //
            // The reply endpoint is a long poll, and the channel server drops
            // an idle connection well before Claude finishes thinking. Re-
            // issuing the GET resumes the wait: the server holds the pending
            // reply until it is collected or its own ten-minute timer expires,
            // so the request is idempotent. Only transport failures are
            // retried — a 404 for an expired message comes back as a status.
            const reply = yield* readResponse(
              HttpClientRequest.get(`${baseUrl}/replies/${messageId}`).pipe(
                HttpClientRequest.bearerToken(channel.token),
              ),
            ).pipe(
              Effect.retry({
                while: (error) => error._tag === "RequestError",
                schedule: Schedule.spaced(REPLY_POLL_INTERVAL),
              }),
              Effect.timeoutFail({
                duration: REPLY_TIMEOUT,
                onTimeout: () =>
                  fail(`Claude did not reply within ${REPLY_TIMEOUT}.`),
              }),
              Effect.catchTags({
                RequestError: (error) =>
                  fail(`failed while waiting for Claude's reply: ${error.message}`),
                ResponseError: (error) =>
                  fail(`failed while waiting for Claude's reply: ${error.message}`),
              }),
            );

            if (reply.status < 200 || reply.status >= 300) {
              return yield* fail(
                `Claude channel reply failed (${describeResponse(reply.status, reply.body)})`,
              );
            }

            const text = yield* decodeReply(reply.body).pipe(
              Effect.map((reply) => reply.text.trim()),
              Effect.mapError((error) =>
                fail(`failed to decode Claude's reply: ${error.message}`),
              ),
            );
            if (text.length === 0) {
              return yield* fail("Claude returned an empty reply.");
            }
            return text;
          }

          return yield* fail(
            `Could not reach a OnlyDiffs Claude channel for this repository. Restart Claude Code with the OnlyDiffs channel enabled.${
              lastError === null ? "" : ` Last error: ${lastError}`
            }`,
          );
        });

      return { send } as const;
    }),
    dependencies: [RepoConfig.Default],
  },
) {}
