#! /bin/bash

# If the current script is running in MacOS, print a warning
if [[ "$OSTYPE" == "darwin"* ]]; then
    # Define the realpath function, as MacOS doesn't have it
    realpath() {
        OURPWD=$PWD
        cd "$(dirname "$1")"
        LINK=$(readlink "$(basename "$1")")
        while [ "$LINK" ]; do
            cd "$(dirname "$LINK")"
            LINK=$(readlink "$(basename "$1")")
        done
        REALPATH="$PWD/$(basename "$1")"
        cd "$OURPWD"
        echo "$REALPATH"
    }

fi

PROJECT_ROOT=$(realpath $(dirname `realpath $0`)/..)

probe-rs erase --chip nrf52840_xxAA --allow-erase-all

probe-rs download --verify --binary-format hex --chip nRF52840_xxAA $PROJECT_ROOT/softdevice/s140_nrf52_7.3.0_softdevice.hex