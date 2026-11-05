// SPDX-License-Identifier: Zlib
// Sounda ARM7 core, adapted from the BlocksDS default Maxmod core.
// Copyright (C) 2005 Michael Noland (joat)
// Copyright (C) 2005 Jason Rogers (Dovoto)
// Copyright (C) 2005-2015 Dave Murphy (WinterMute)
// Copyright (C) 2023 Antonio Niño Díaz
// Altered for Sounda: report the DSi headphone GPIO on FIFO_USER_01.
// The original license is in docs/blocksds-arm7-license.txt.
#include <nds.h>
#include <nds/arm7/gpio.h>
#include <maxmod7.h>

static volatile bool exiting;
static uint32_t last_jack = UINT32_MAX;

static uint32_t jack(void)
{
    return isDSiMode() && (REG_GPIO_DATA & GPIO_HEADPHONE_CONNECT) ? 1u : 0u;
}
static void jack_request(uint32_t value, void *context)
{
    (void)value;
    (void)context;
    fifoSendValue32(FIFO_USER_01, jack());
}
static void on_vblank(void)
{
    inputGetAndSend();
    uint32_t state = jack();
    if (state != last_jack)
    {
        if (fifoSendValue32(FIFO_USER_01, state)) last_jack = state;
    }
}
static void on_power(void) { exiting = true; }

int main(void)
{
    enableSound();
    readUserSettings();
    ledBlink(0);
    touchInit();
    irqInit();
    fifoInit();
    installSoundFIFO();
    installSystemFIFO();
    if (isDSiMode()) installCameraFIFO();
    mmInstall(FIFO_MAXMOD);
    fifoSetValue32Handler(FIFO_USER_01, jack_request, NULL);
    setPowerButtonCB(on_power);
    initClockIRQTimer(LIBNDS_DEFAULT_TIMER_RTC);
    irqSet(IRQ_VBLANK, on_vblank);
    irqEnable(IRQ_VBLANK);
    while (!exiting) swiWaitForVBlank();
    return 0;
}
