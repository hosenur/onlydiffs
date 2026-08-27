import { createFileRoute } from '@tanstack/react-router'
import { AppNoSelection } from '@/components/app-no-selection'

export const Route = createFileRoute('/_app/diff')({
  component: AppNoSelection,
})
