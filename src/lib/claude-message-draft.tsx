import { createContext, useCallback, useContext, useMemo, useState } from 'react'
import type { Dispatch, ReactNode, SetStateAction } from 'react'

interface ClaudeMessageDraftValue {
  draft: string
  setDraft: Dispatch<SetStateAction<string>>
}

const ClaudeMessageDraftContext = createContext<ClaudeMessageDraftValue | null>(null)
const AddClaudeReferenceContext = createContext<((reference: string) => void) | null>(null)

export function ClaudeMessageDraftProvider({ children }: { children: ReactNode }) {
  const [draft, setDraft] = useState('')
  const addReference = useCallback((reference: string) => {
    setDraft((current) => {
      if (!current.trim()) return reference
      if (current.split(/\r?\n/).some((line) => line.trim() === reference)) return current
      return `${current}${current.endsWith('\n') ? '' : '\n'}${reference}`
    })
  }, [])
  const draftValue = useMemo(() => ({ draft, setDraft }), [draft])

  return (
    <AddClaudeReferenceContext.Provider value={addReference}>
      <ClaudeMessageDraftContext.Provider value={draftValue}>
        {children}
      </ClaudeMessageDraftContext.Provider>
    </AddClaudeReferenceContext.Provider>
  )
}

export function useClaudeMessageDraft() {
  const value = useContext(ClaudeMessageDraftContext)
  if (!value) throw new Error('useClaudeMessageDraft must be used within ClaudeMessageDraftProvider')
  return value
}

export function useAddClaudeReference() {
  const addReference = useContext(AddClaudeReferenceContext)
  if (!addReference) {
    throw new Error('useAddClaudeReference must be used within ClaudeMessageDraftProvider')
  }
  return addReference
}
