# Open Build Service: next distribution stage

Open Build Service (OBS) is a service that builds source packages for selected Linux
distributions and CPU architectures, then publishes package repositories. It is a
separate service from GitHub Actions. The public openSUSE instance is
[build.opensuse.org](https://build.opensuse.org/).

A Meshcraft OBS project would contain the release source and packaging recipes.
For example, one source version could produce Ubuntu/Debian DEBs and Fedora/openSUSE
RPMs. Each selected distribution has its own build environment and dependencies.
The project publishes repositories from which users can install and receive updates
with their usual package manager. Users add the appropriate repository once.

## What the current GitHub workflow does

The tagged Release workflow builds optimized Linux binaries against the audited
Ubuntu 18.04 baseline. It then wraps each portable payload in DEB and RPM installers
and attaches them to the GitHub release. These downloads install directly, but do
not configure an update repository. The generated RPM spec repackages a completed
binary payload; it is not an OBS source-build recipe.

## What OBS would add

1. Create a project under the publisher's OBS account.
2. Choose the distribution releases and architectures to support.
3. Add Debian source packaging and an RPM source-build spec, together with the
   matching Meshcraft source archive. Rust dependency vendoring and the pinned
   toolchain must be made available to the isolated build environments.
4. Have OBS build, check, sign and publish the packages to that project's repositories.
5. Document the generated repository URLs and installation instructions.
6. Connect a successful tagged GitHub release to an OBS source update, with explicit
   stable/testing publication policy and credentials supplied by the publisher.

The account, target distro list and publishing policy are still to be chosen.
There is no OBS upload, account provisioning or repository publication in this phase.

Official references:

- [OBS beginner's guide](https://openbuildservice.org/help/manuals/obs-user-guide/art-obs-bg)
- [Supported recipes and package formats](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-package-formats)
- [Repository publishing and project concepts](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-concepts)
