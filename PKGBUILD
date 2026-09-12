pkgname=helix-fork
pkgver=25.07.1.20260912.114874c7
pkgrel=1
pkgdesc="Personal Helix fork"
arch=('x86_64')
license=('MPL-2.0')
makedepends=('cargo' 'librsvg' 'git')
provides=('hx')
conflicts=('helix' 'helix-git')
options=(!lto)
source=()

_bin=hx
_lib=/usr/lib/helix
_rt=${_lib}/runtime

_runtime_hash() {
  find runtime -type f ! -path 'runtime/grammars/sources/*' -print0 \
    | sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}'
}

_calver() {
  cd "$startdir"
  local v major minor patch
  v=$(grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
  IFS=. read -r major minor patch <<< "$v"
  printf '%s.%02d.%s\n' "$major" "$minor" "$patch"
}

_fork_version_display() {
  cd "$startdir"
  local ver date hash
  ver=$(_calver)
  date=$(git log -1 --format=%cs | tr -d '-')
  hash=$(git rev-parse --short=8 HEAD)
  printf '%s-%s-%s\n' "$ver" "$date" "$hash"
}

pkgver() {
  _fork_version_display | tr '-' '.'
}

build() {
  cd "$startdir"
  export HELIX_PKGVER="$(_fork_version_display)"
  export CARGO_TARGET_DIR="$startdir/target"
  export RUSTFLAGS="${RUSTFLAGS} -C target-cpu=native"
  cargo build --locked --profile opt -p helix-term
}

package() {
  cd "$startdir"

  rm -rf runtime/grammars/sources
  local hash=$(_runtime_hash)
  local cache="$startdir/.pkg-cache/runtime-$hash"

  install -dm755 "$pkgdir$_lib"
  if [[ -d "$cache" ]]; then
    cp -a "$cache/." "$pkgdir$_lib/runtime/"
  else
    rm -rf "$startdir/.pkg-cache"
    mkdir -p "$cache"
    cp -a runtime "$cache/"
    cp -a runtime "$pkgdir$_lib/"
  fi

  install -Dm755 "target/opt/$_bin" "$pkgdir$_lib/$_bin"
  install -Dm755 /dev/stdin "$pkgdir/usr/bin/$_bin" <<EOF
#!/usr/bin/env sh
HELIX_RUNTIME=$_rt exec $_lib/$_bin "\$@"
EOF

  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm644 contrib/Helix.desktop "$pkgdir/usr/share/applications/Helix.desktop"
  install -Dm644 contrib/Helix.appdata.xml "$pkgdir/usr/share/appdata/Helix.appdata.xml"
  install -Dm644 logo.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/helix.svg"
  rsvg-convert -w 256 -h 256 logo.svg \
    | install -Dm644 /dev/stdin "$pkgdir/usr/share/pixmaps/helix.png"

  install -Dm644 contrib/completion/hx.zsh "$pkgdir/usr/share/zsh/site-functions/_hx"
  install -Dm644 contrib/completion/hx.bash "$pkgdir/usr/share/bash-completion/completions/hx.bash"
  install -Dm644 contrib/completion/hx.fish "$pkgdir/usr/share/fish/vendor_completions.d/hx.fish"
}
