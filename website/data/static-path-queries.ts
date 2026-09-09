import supabaseAdmin from '~/lib/supabase-admin'

// The queries in this file should only be used for static path generation,
// as they bypass RLS. They will not work client side.

export async function getAllProfiles() {
  const [
    { data: organizations, error: organizationsError },
    { data: accounts, error: accountsError },
  ] = await Promise.all([
    supabaseAdmin
      .from('organizations')
      .select('handle')
      .order('created_at', { ascending: false })
      .limit(500)
      .returns<{ handle: string }[]>(),
    supabaseAdmin
      .from('accounts')
      .select('handle')
      .order('created_at', { ascending: false })
      .limit(500)
      .returns<{ handle: string }[]>(),
  ])

  if (organizationsError) {
    console.error(
      'Failed to list organizations for static paths',
      organizationsError
    )
  }
  if (accountsError) {
    console.error('Failed to list accounts for static paths', accountsError)
  }

  return [...(organizations ?? []), ...(accounts ?? [])]
}

export async function getAllPackages() {
  const { data, error } = await supabaseAdmin
    .from('packages')
    .select('handle,partial_name')
    .order('created_at', { ascending: false })
    .limit(1000)
    .returns<{ handle: string; partial_name: string }[]>()

  if (error) {
    console.error('Failed to list packages for static paths', error)
  }

  return data ?? []
}
