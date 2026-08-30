# GitHub repository-specific icons

Research date: 2026-08-31

## Answer

GitHub does not expose a repository-specific avatar for the icon position shown in the screenshot. That image belongs to the repository owner. In the example, it is the `spacedriveapp` organization avatar, returned as `avatar_url` by GitHub's organization API. Every repository owned by that organization gets the same avatar in that position. [GitHub organization API response](https://api.github.com/orgs/spacedriveapp)

GitHub does support one repository-specific image: the **social preview image**. It is a wide Open Graph card for links shared outside GitHub. It does not replace the owner avatar in the repository header.

## What GitHub supports

Repository administrators can upload a social preview under **Settings > Social preview > Edit > Upload an image**. GitHub accepts PNG, JPG, or GIF files under 1 MB. It recommends at least 640 by 320 pixels, with 1280 by 640 pixels preferred. GitHub says images uploaded to private repositories cannot be shared publicly. [GitHub Docs: Customizing your repository's social media preview](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/customizing-your-repositorys-social-media-preview)

The GraphQL `Repository` object exposes two useful read-only fields:

- `openGraphImageUrl`: "The image used to represent this repository in Open Graph data."
- `usesCustomOpenGraphImage`: whether the repository has a custom Open Graph image instead of the owner's avatar.

These fields are present in GitHub's published schema. [GitHub public GraphQL schema](https://github.com/github/docs/blob/main/src/graphql/data/fpt/schema.docs.graphql#L55359-L55362) [Custom-image flag](https://github.com/github/docs/blob/main/src/graphql/data/fpt/schema.docs.graphql#L56111-L56114)

A query would look like this:

```graphql
query RepositoryImage($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    openGraphImageUrl
    usesCustomOpenGraphImage
  }
}
```

GitHub's GraphQL API requires authentication, including for public data. OnlyDiffs would need a GitHub token or a separate GitHub sign-in flow to use this documented interface. [GitHub Docs: Forming calls with GraphQL](https://docs.github.com/en/graphql/guides/forming-calls-with-graphql#communicating-with-graphql)

GitHub's documented REST repository response has an `owner` with `avatar_url`, but no repository avatar, icon, or Open Graph image field. The documented repository update request also has no icon or social-preview image parameter. [Get a repository](https://docs.github.com/en/rest/repos/repos#get-a-repository) [Update a repository](https://docs.github.com/en/rest/repos/repos#update-a-repository)

The official social-preview documentation only describes uploading through repository settings. The published REST and GraphQL schemas do not expose a mutation for uploading that image.

## What can be fetched without authentication

A public repository page includes an `og:image` meta tag. When a custom social preview exists, it commonly points at `repository-images.githubusercontent.com`. Without one, GitHub commonly returns a generated card from `opengraph.githubassets.com`.

This can be scraped without a token, but GitHub does not document repository HTML as an image API. The markup and URL form may change. It also returns a 2:1 card, not a square icon, so cropping it can cut off text or artwork. It is a poor default for the 32 px OnlyDiffs project rail.

Files such as `.github/logo.png`, a README logo, a web favicon, and package icons are project conventions rather than Git or GitHub repository metadata. Spacedrive happens to have `.github/logo.png`, but GitHub does not use that file for the highlighted header avatar. [Spacedrive logo file](https://github.com/spacedriveapp/spacedrive/blob/main/.github/logo.png)

## Recommendation for OnlyDiffs

Do not scrape GitHub or guess among repository assets. Add an explicit OnlyDiffs project icon setting, with the Nucleo cube as the fallback.

A practical lookup order is:

1. A local per-project override stored in OnlyDiffs state. This works for private repositories and does not change the checkout.
2. An optional committed project setting, for teams that want to share the icon. For example, `.onlydiffs/project.json` could contain `{ "icon": "./assets/project-icon.png" }`.
3. The Nucleo isometric cube fallback.

If GitHub integration is added later, OnlyDiffs can offer the custom social preview as an import source only when `usesCustomOpenGraphImage` is true. It should not silently use generated Open Graph cards or owner avatars as project icons.

## Conclusion

GitHub cannot assign a unique avatar to the repository header. Its native repository-specific image is the social preview card, which has the wrong shape and purpose for the OnlyDiffs rail. A per-project OnlyDiffs setting is the reliable option.
