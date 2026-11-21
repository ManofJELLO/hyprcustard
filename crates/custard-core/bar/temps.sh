# Finds the sensor the bar's CPU or GPU temperature reads:
#
#   sh temps.sh cpu|gpu [sysfs root]
#
# Prints one tab-separated line, or nothing when there is no sensor:
#
#   file    <temp*_input path>  <device folder, or ->  <sensor's name>
#   nvidia  -                   <device folder, or ->  <sensor's name>
#
# The device folder is where the bar checks that a GPU is awake before it
# reads, so a sleeping laptop GPU is left asleep. NVIDIA's own driver has no
# kernel sensor, so its temperature comes from nvidia-smi.

kind=$1
sys=${2:-/sys}

# The hwmon folders whose driver is called $1.
hwmons() {
    for h in "$sys"/class/hwmon/hwmon*; do
        [ "$(cat "$h/name" 2>/dev/null)" = "$1" ] && echo "$h"
    done
}

# In hwmon folder $1, the input of the first of the labels after it that the
# sensor has, else its first input. Sets $input and $label.
pick() {
    d=$1
    shift
    for want in "$@"; do
        for l in "$d"/temp*_label; do
            [ -r "$l" ] || continue
            if [ "$(cat "$l" 2>/dev/null)" = "$want" ] && [ -r "${l%_label}_input" ]; then
                input=${l%_label}_input
                label=$want
                return 0
            fi
        done
    done
    for i in "$d"/temp*_input; do
        if [ -r "$i" ]; then
            input=$i
            label=$(cat "${i%_input}_label" 2>/dev/null)
            return 0
        fi
    done
    return 1
}

# Print what was found in hwmon folder $1 for driver $2, and stop.
found() {
    device=-
    [ -e "$1/device/power/runtime_status" ] && device=$1/device
    printf 'file\t%s\t%s\t%s\n' "$input" "$device" "$2${label:+ $label}"
    exit 0
}

# Try each hwmon folder for driver $1, with the labels after it best first.
try() {
    n=$1
    shift
    for h in $(hwmons "$n"); do
        pick "$h" "$@" && found "$h" "$n"
    done
}

case $kind in
cpu)
    # AMD: Tdie where there is one, since some Ryzens add an offset to Tctl.
    try k10temp Tdie Tctl
    try zenpower Tdie Tctl
    # Intel: the whole package rather than one core.
    try coretemp "Package id 0"
    # ARM boards.
    try cpu_thermal
    try cpu-thermal
    try soc_thermal
    for z in "$sys"/class/thermal/thermal_zone*; do
        t=$(cat "$z/type" 2>/dev/null)
        case $t in
        x86_pkg_temp | cpu* | *cpu-thermal | *cpu_thermal)
            [ -r "$z/temp" ] && { printf 'file\t%s\t-\t%s\n' "$z/temp" "$t"; exit 0; } ;;
        esac
    done
    ;;
gpu)
    # A card with NVIDIA's own driver is the discrete one, so it comes first.
    if [ -d "$sys/module/nvidia" ] && command -v nvidia-smi >/dev/null 2>&1; then
        device=-
        for d in "$sys"/bus/pci/drivers/nvidia/[0-9a-f]*:*; do
            [ -e "$d/power/runtime_status" ] && { device=$d; break; }
        done
        printf 'nvidia\t-\t%s\t%s\n' "$device" "nvidia-smi"
        exit 0
    fi
    # An AMD card with a junction sensor is a discrete one, not the
    # processor's own graphics.
    for h in $(hwmons amdgpu); do
        for l in "$h"/temp*_label; do
            if [ "$(cat "$l" 2>/dev/null)" = junction ]; then
                pick "$h" edge junction && found "$h" amdgpu
            fi
        done
    done
    try nouveau
    try amdgpu edge junction
    try radeon
    try xe pkg
    try i915
    ;;
esac
exit 0
