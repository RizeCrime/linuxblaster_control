{
  description = "SoundBlasterX G6 — automatic ALSA mixer fix";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }: {

    nixosModules.default = { config, lib, pkgs, ... }:
      let
        cfg = config.hardware.soundblasterx-g6;

        fixScript = pkgs.writeShellScript "soundblasterx-g6-alsa-fix" ''
          CARD_NAME="Sound BlasterX G6"
          LOG_TAG="g6-alsa-fix"
          MAX_RETRIES=10
          RETRY_DELAY=1

          log() {
            ${pkgs.util-linux}/bin/logger -t "$LOG_TAG" "$1"
          }

          card_index=""
          attempt=0
          while [ "$attempt" -lt "$MAX_RETRIES" ]; do
            card_index=$(${pkgs.alsa-utils}/bin/aplay -l 2>/dev/null \
              | ${pkgs.gnugrep}/bin/grep "$CARD_NAME" \
              | head -n1 \
              | ${pkgs.gnused}/bin/sed 's/^card \([0-9]*\).*/\1/')
            if [ -n "$card_index" ]; then
              break
            fi
            attempt=$((attempt + 1))
            sleep "$RETRY_DELAY"
          done

          if [ -z "$card_index" ]; then
            log "ERROR: Could not find '$CARD_NAME' after $MAX_RETRIES attempts"
            exit 1
          fi

          log "Found '$CARD_NAME' as card $card_index, applying fixes..."

          # The critical fix: set correct capture source
          ${pkgs.alsa-utils}/bin/amixer -c "$card_index" -q \
            cset name='PCM Capture Source' 'External Mic' 2>/dev/null

          ${lib.optionalString cfg.muteUnusedInputs ''
            # Mute unused playback inputs
            for input in 'External Mic' 'Line In' 'S/PDIF In'; do
              ${pkgs.alsa-utils}/bin/amixer -c "$card_index" -q \
                cset "name=''${input} Playback Volume" 0 2>/dev/null
              ${pkgs.alsa-utils}/bin/amixer -c "$card_index" -q \
                cset "name=''${input} Playback Switch" off 2>/dev/null
            done

            # Mute unused capture inputs
            for input in 'Line In' 'S/PDIF In' 'What U Hear'; do
              ${pkgs.alsa-utils}/bin/amixer -c "$card_index" -q \
                cset "name=''${input} Capture Volume" 0 2>/dev/null
              ${pkgs.alsa-utils}/bin/amixer -c "$card_index" -q \
                cset "name=''${input} Capture Switch" off 2>/dev/null
            done
          ''}

          log "Successfully applied ALSA fixes for card $card_index"
        '';
      in {
        options.hardware.soundblasterx-g6 = {
          enable = lib.mkEnableOption "SoundBlasterX G6 ALSA fix";

          muteUnusedInputs = lib.mkOption {
            type = lib.types.bool;
            default = true;
            description = ''
              Mute unused playback and capture inputs (Line In, S/PDIF, What U Hear)
              to prevent feedback and bleed. Disable if you actually use these inputs.
            '';
          };
        };

        config = lib.mkIf cfg.enable {
          # Ensure alsa-utils is available system-wide
          environment.systemPackages = [ pkgs.alsa-utils ];

          services.udev.extraRules = ''
            SUBSYSTEM=="sound", ACTION=="add", DEVPATH=="*/controlC*", \
              ATTRS{idVendor}=="041e", ATTRS{idProduct}=="3256", \
              RUN+="${pkgs.bash}/bin/sh -c '${fixScript} &'"
          '';
        };
      };
  };
}
