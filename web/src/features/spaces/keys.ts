import type { QueryClient } from '@tanstack/react-query'

/** Query keys owned by the spaces slice: list, detail, members, usage. */
export const spaceKeys = {
  list: ['spaces'] as const,
  detail: (spaceId: string) => ['space', spaceId] as const,
  members: (spaceId: string) => ['space-members', spaceId] as const,
  usage: (spaceId: string) => ['space-usage', spaceId] as const,
}

/** Every space mutation may change the list, the detail, members or usage. */
export function invalidateSpaceQueries(queryClient: QueryClient, spaceId: string): void {
  void queryClient.invalidateQueries({ queryKey: spaceKeys.list })
  void queryClient.invalidateQueries({ queryKey: spaceKeys.detail(spaceId) })
  void queryClient.invalidateQueries({ queryKey: spaceKeys.members(spaceId) })
  void queryClient.invalidateQueries({ queryKey: spaceKeys.usage(spaceId) })
}
