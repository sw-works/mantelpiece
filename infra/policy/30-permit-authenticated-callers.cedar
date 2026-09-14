// Everything else on the Gateway (reads, reversible writes, proposals) is allowed for any
// authenticated principal; the API scopes every call to the caller's household.
permit(principal, action, resource);
