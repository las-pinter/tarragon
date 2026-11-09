#!/bin/sh
# Download the PSD corpus used by the real-file smoke tests, and check each
# file against a pinned sha256.
#
# Usage: sh tests/psd-corpus/fetch.sh [DIR]
# DIR defaults to $TARRAGON_PSD_CORPUS. The layout is flat: all files in DIR.
#
# Sources, each pinned to one commit so the bytes cannot change:
#   ag-psd     github.com/Agamnentzar/ag-psd   a74425db6584cc07b1cc135e370916cb4eccfb32
#   Krita      github.com/KDE/krita            f260d4523839a306f3b856b7158d5ca78b1c9b9f
#   psd-tools  github.com/psd-tools/psd-tools  95139b884290f03b172a78191f49ee72795faf8e
#
# Licences: only the repository licences are known (ag-psd MIT, Krita
# GPL-3.0, psd-tools MIT). Upstream does not state the licence of each binary
# file. For that reason the files are never committed.
set -eu

usage() {
    echo "usage: sh $0 [DIR]   (DIR defaults to \$TARRAGON_PSD_CORPUS)" >&2
}

dir="${1:-${TARRAGON_PSD_CORPUS:-}}"
if [ -z "$dir" ]; then
    usage
    exit 2
fi

if command -v sha256sum >/dev/null 2>&1; then
    hash_of() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
    hash_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
    echo "error: neither sha256sum nor shasum is installed" >&2
    exit 2
fi

mkdir -p "$dir"

agpsd="https://raw.githubusercontent.com/Agamnentzar/ag-psd/a74425db6584cc07b1cc135e370916cb4eccfb32/test/read"
krita="https://raw.githubusercontent.com/KDE/krita/f260d4523839a306f3b856b7158d5ca78b1c9b9f/plugins/impex/psd/tests/data"
pstools="https://raw.githubusercontent.com/psd-tools/psd-tools/95139b884290f03b172a78191f49ee72795faf8e/tests/psd_files/layers"

count=0

# A file gets its final name only after its hash matches, so a bad or
# partial download is never mistaken for a good file on the next run.
fetch() {
    name="$1"
    url="$2"
    want="$3"
    final="$dir/$name"
    part="$final.part"
    if [ -f "$final" ] && [ "$(hash_of "$final")" = "$want" ]; then
        echo "ok (kept) $name"
    else
        echo "download $name"
        if ! curl --proto '=https' --fail --silent --show-error --location --max-time 120 \
            --output "$part" "$url"; then
            rm -f "$part"
            echo "error: download failed for $name ($url)" >&2
            exit 1
        fi
        got="$(hash_of "$part")"
        if [ "$got" != "$want" ]; then
            rm -f "$part"
            echo "error: sha256 mismatch for $name" >&2
            echo "  expected $want" >&2
            echo "  got      $got" >&2
            exit 1
        fi
        mv -f "$part" "$final"
    fi
    count=$((count + 1))
}

fetch 16bits.psd "$agpsd/16bits/src.psd" 0984374b18fd7e17993b3f0b391b41d956d8257a6ba933f0a45d3ad17b4da282
fetch 16bits.png "$agpsd/16bits/canvas.png" 8d6b8847f5203272c5d621921f8790bf17193257605e0bb09a0202d2c62c28bb
fetch 32bits.psd "$agpsd/32bits/src.psd" c901a22a89dbe9f0a1601f2f122bf269ed55af837b8720619475937b0ce48eb8
fetch 32bits.png "$agpsd/32bits/canvas.png" 323e805a7fd198a1c4680e27deb20817dd2450c78f7eeb4a0dfcc8d90dbe31be
fetch cmyk.psd "$agpsd/cmyk/src.psd" 4f65d52bbe87c656b8a5927d8c3f8cbaa144f15c9ba8c4ff843bcdb90f2676c1
fetch cmyk.png "$agpsd/cmyk/canvas.png" 91e63f867011709188a066f84f84cb95a3751315e63a1c46f7e8b56f28b0f2c3
fetch psb.psb "$agpsd/psb/src.psb" 3aae71e57e7e6044c3df63ee9292151007eec7e63c44587e79c607d0eda34449
fetch psb.png "$agpsd/psb/canvas.png" 15e5870356f2f19e15f19659eb3448f4b3843e9876f7c65bad81978d01a7f804
fetch adjustment-layers.psd "$agpsd/adjustment-layers/src.psd" 60a8e5f4226345bc5adc8ea6a1ce2f65c36ff03000fdb94be98c8931a71c7bc2
fetch adjustment-layers.png "$agpsd/adjustment-layers/canvas.png" 57b624730a26f79872a23296c4dc1749852af322f7e849259fb32b3fbe4d268f
fetch effects.psd "$agpsd/effects/src.psd" b7f467529c585537fd982ab9c0879b18b0d05340e72b06d705b324c11fb00d25
fetch effects.png "$agpsd/effects/canvas.png" 1c1ab8ea4165ce00dc7867a04b373e1572149bc7791ff29825e066e9a4363826
fetch vector_mask.psd "$krita/sources/vector_mask.psd" 9e174b6e72af358c9a9cc67075b71dcbbb4ac81fd96bf3c033439783e6c278c9
fetch masks.psd "$krita/sources/masks.psd" 60185c347936e427e890864f1608c483f38be1e1ecc22d3605a9468cd0a7826d
fetch masks.png "$krita/reference_results/masks.png" 8d0f30d532c1975ae2e6be59f80327b6f6d45388496b3f4973ab985bfc04295b
fetch pattern-fill.psd "$pstools/pattern-fill.psd" f726a18877069161884ec933c5ea4962918f45de8317a47abe6bb9075baf059f

echo "$count files verified in $dir"
