import { useEffect, useState } from 'react'
import { Button } from '@onlydiffs/ui/button'
import { useSsh } from '@/lib/ssh'

/**
 * The two questions connecting can ask.
 *
 * Both are modal on purpose: ssh is blocked on the answer, and a prompt that
 * can be scrolled away is one that leaves a process waiting until it times out.
 */
export function SshDialogs() {
  const { prompt, unknownHost, answer, cancel, trustAndConnect, dismissUnknownHost } = useSsh()
  const [value, setValue] = useState('')

  // A new question is a new answer; carrying the last one across would submit
  // a passphrase to whatever asked next.
  useEffect(() => setValue(''), [prompt?.id])

  if (unknownHost) {
    return (
      <Backdrop>
        <h2 className="font-medium text-base">
          {unknownHost.alias} has not been connected to before
        </h2>
        <p className="text-muted-fg text-sm">
          Nothing has vouched for this machine yet. Check the fingerprint against what
          whoever runs the host told you — this is the one moment where a
          machine-in-the-middle would be caught.
        </p>
        <dl className="flex flex-col gap-1.5 rounded-lg border border-border p-3 font-mono text-xs">
          <Row label="Host">
            {unknownHost.hostname}
            {unknownHost.port === null || unknownHost.port === 22 ? '' : `:${unknownHost.port}`}
          </Row>
          <Row label="Key type">{unknownHost.keyType}</Row>
          <Row label="Fingerprint">
            <span className="break-all">{unknownHost.fingerprint}</span>
          </Row>
        </dl>
        <p className="text-muted-fg text-xs">
          Approving adds it to your <span className="font-mono">~/.ssh/known_hosts</span>, so
          every ssh client on this machine trusts it — not just OnlyDiffs.
        </p>
        <div className="flex justify-end gap-2">
          <Button intent="outline" onPress={dismissUnknownHost}>
            Cancel
          </Button>
          <Button onPress={() => void trustAndConnect()}>I recognise it — connect</Button>
        </div>
      </Backdrop>
    )
  }

  if (!prompt) return null

  return (
    <Backdrop>
      <h2 className="font-medium text-base">ssh is asking</h2>
      {/* ssh's own words, verbatim: it names the key or the account, and
          paraphrasing would lose which one it means. */}
      <p className="font-mono text-muted-fg text-sm">{prompt.text}</p>
      <form
        className="flex flex-col gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          answer(value)
        }}
      >
        {/* eslint-disable-next-line jsx-a11y/no-autofocus -- ssh is blocked on
            this field; anything else on screen is unusable until it is answered. */}
        <input
          autoFocus
          type={prompt.isSecret ? 'password' : 'text'}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              cancel()
            }
          }}
          aria-label={prompt.text}
          autoComplete="off"
          spellCheck={false}
          className="w-full rounded-lg border border-border bg-bg px-3 py-2 font-mono text-sm outline-hidden focus:border-primary"
        />
        <div className="flex justify-end gap-2">
          <Button intent="outline" type="button" onPress={cancel}>
            Cancel
          </Button>
          <Button type="submit" isDisabled={value.length === 0}>
            Continue
          </Button>
        </div>
      </form>
    </Backdrop>
  )
}

function Backdrop({ children }: { children: React.ReactNode }) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-fg/20 p-6 backdrop-blur-sm">
      <div
        role="dialog"
        aria-modal="true"
        className="flex w-full max-w-md flex-col gap-4 rounded-xl border border-border bg-overlay p-5 shadow-lg"
      >
        {children}
      </div>
    </div>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3">
      <dt className="w-20 shrink-0 text-muted-fg">{label}</dt>
      <dd className="min-w-0 flex-1">{children}</dd>
    </div>
  )
}
