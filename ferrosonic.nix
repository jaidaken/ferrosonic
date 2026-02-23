{ lib
, rustPlatform
, fetchFromGitHub
, pkg-config
, makeWrapper
, openssl
, mpv
, pipewire
, wireplumber
, cava
}:

rustPlatform.buildRustPackage rec {
  pname = "ferrosonic";
  version = "0.2.2";

  src = fetchFromGitHub {
    owner = "jaidaken";
    repo = "ferrosonic";
    rev = "v${version}";
    hash = "sha256-cqmu+PDWKnSHYzV6TOVFwDdHEHjsgalIveEhEK87fi8=";
  };

  cargoHash = "sha256-vari4D3gHGYOOmVRQaEtmTkhT3E+fTnZgNZSQrnG0bc=";

  nativeBuildInputs = [ pkg-config makeWrapper ];
  buildInputs = [ openssl ];

  doCheck = false;

  postInstall = ''
    wrapProgram "$out/bin/ferrosonic" \
      --prefix PATH : ${lib.makeBinPath [ mpv pipewire wireplumber cava ]}
  '';
}
