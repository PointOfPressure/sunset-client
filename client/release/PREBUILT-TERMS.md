# Terms of the Sunset prebuilt client

These terms cover the prebuilt Sunset client: the game package `sunset-<version>-<horizon>-<platform>.tar.zst`
(the game folder the Sunset launcher installs) and the files released with it. The Sunset launcher and the Sunset
source are covered by Sunset's `LICENSE`.

1. **Sunset.** Sunset's own code in the package (`libsunset_core.so` and `horizon-native-mods/sunset/`) is
   licensed under the PolyForm Strict License 1.0.0 with the additional permissions in Sunset's licence
   (`SUNSET-LICENSE.txt`), except as section 3 says. Where Sunset's mixins are woven into Horizon's functions
   (`libhorizon_patch.so` and its source on the fork) they are part of a modified Horizon: section 2.
2. **Horizon.** Every other program and library in the package, and `libhorizon_patch.so` (Horizon's own
   functions with Sunset's mixins woven in), is Horizon or a modified version of it, licensed under the GNU Lesser
   General Public License, version 3 or later (`LICENSE-LGPL-3.0.txt` and `LICENSE-GPL-3.0.txt`).
   `HORIZON-NOTICE.txt` names its copyright holders, the exact source this build is made from and how to rebuild
   or relink it. Nothing in Sunset's licence or in these terms limits your rights to Horizon under the LGPL.
3. **Modifying the Horizon part.** You may modify the Horizon part of the prebuilt client for your own use,
   including replacing its Horizon libraries with modified versions and relinking `libhorizon_patch.so` (the relink
   kit released beside the package), and use Sunset with those modifications; and you may reverse engineer the
   prebuilt client to debug such modifications. This permission is given for section 4 of the GNU LGPL version 3
   and covers everything that section asks of the terms of a combined work.
4. **Third-party components** keep their own licences: `THIRD-PARTY-LICENSES.txt`.

In the game folder these files are in `licences/`; each release also carries them as separate files.
