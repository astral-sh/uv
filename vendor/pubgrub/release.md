# Creation of a new release

This is taking the 0.2.1 release as an example.

## GitHub stuff

- Checkout the prep-v0.2.1 branch
- Update the release date in the changelog and push to the PR.
- Squash merge the PR to the dev branch
- Check that the merged PR is passing the tests on the dev branch
- Pull the updated dev locally
- Switch to the release branch
- Merge locally dev into release in fast-forward mode, we want to keep the history of commits and the merge point.
- `git tag -a v0.2.1 -m "v0.2.1: mostly perf improvements"`
- (Optional) cryptographically sign the tag
- On GitHub, edit the branch protection setting for release: uncheck include admin, and save
- Push release to github: git push --follow-tags
- Reset the release branch protection to include admins
- On GitHub, create a release from that tag.

## Crates.io stuff

- `cargo publish --dry-run`
- `cargo publish`

## Community stuff

Talk about the awesome new features of the new release online.
