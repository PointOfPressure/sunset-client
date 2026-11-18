# Prebuilt client releases (placeholder)

Sunset 2.0 ships a prebuilt, signed Horizon+Sunset executable per platform per pin; the launcher downloads and
verifies it. The scripts here will do that; they are stubs until the launcher run lands.

- `build.sh <platform>`: checks out the PointOfPressure/horizon branch for the pin in `../horizon-native.json`, applies
  `../horizon-patches/`, applies Sunset (`../mixins/`) with horizon-mixins and builds the client.
- `sign.sh <file>...`: signs the artifacts with minisign (ed25519). The secret key stays on the build machine; the
  public key is baked into the launcher. Releases go to GitHub Releases under `client-vX` tags.
