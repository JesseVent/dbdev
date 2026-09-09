import { PostgrestError } from '@supabase/supabase-js'
import { QueryClient, useQuery, UseQueryOptions } from '@tanstack/react-query'
import supabase from '~/lib/supabase'
import { NonNullableObject } from '~/lib/types'
import { Database } from '../database.types'

export type AllPackagesResponse = NonNullableObject<
  Database['public']['Views']['packages']['Row']
>[]

export async function getAllPackages(signal?: AbortSignal) {
  let query = supabase
    .from('packages')
    .select('*')
    .order('created_at', { ascending: false })

  if (signal) {
    query = query.abortSignal(signal)
  }

  const { data, error } = await query.returns<AllPackagesResponse>()

  if (error) {
    throw error
  }

  return data ?? []
}

export type AllPackagesData = Awaited<ReturnType<typeof getAllPackages>>
export type AllPackagesError = PostgrestError

export const useAllPackagesQuery = <TData = AllPackagesData>({
  enabled = true,
  ...options
}: Omit<
  UseQueryOptions<AllPackagesData, AllPackagesError, TData>,
  'queryKey' | 'queryFn'
> = {}) =>
  useQuery<AllPackagesData, AllPackagesError, TData>({
    queryKey: ['packages', 'all'],
    queryFn: ({ signal }) => getAllPackages(signal),
    enabled,
    ...options,
  })

export const prefetchAllPackages = (client: QueryClient) => {
  return client.prefetchQuery({
    queryKey: ['packages', 'all'],
    queryFn: ({ signal }) => getAllPackages(signal),
  })
}
