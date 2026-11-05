// Hardware and newlib ABI adapter. Browser, demuxing, firmware validation,
// playback state and graphics are implemented in Rust.
#ifdef SOUNDA_AUDIO_TEST
#include "../tests/audio_sdk.h"
static int16_t decoded_left[1024];
static int16_t decoded_right[1024];
volatile uint32_t sounda_diagnostics[40] = { 0x534e4441u, 3u };
#else
#include <nds.h>
#include <nds/arm9/teak/dsp.h>
#include <nds/nwram.h>
#include <fat.h>
#include <maxmod9.h>
#include <dirent.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

static int backgrounds[2];
static volatile uint32_t headphones;
static void headphone_status(uint32_t status, void *unused)
{
    (void)unused;
    headphones = status;
}

// Stable ARM9 debugger telemetry. See docs/verification.md for the layout.
volatile uint32_t sounda_diagnostics[40] = { 0x534e4441u, 3u };

void sounda_init(void)
{
    defaultExceptionHandler();
    powerOn(POWER_ALL_2D);
    lcdMainOnTop();
    videoSetMode(MODE_5_2D);
    videoSetModeSub(MODE_5_2D);
    vramSetBankA(VRAM_A_MAIN_BG);
    vramSetBankC(VRAM_C_SUB_BG);
    backgrounds[0] = bgInit(3, BgType_Bmp16, BgSize_B16_256x256, 0, 0);
    backgrounds[1] = bgInitSub(3, BgType_Bmp16, BgSize_B16_256x256, 0, 0);
    setBrightness(3, 0);
    keysSetRepeat(24, 5);
    fifoSetValue32Handler(FIFO_USER_01, headphone_status, NULL);
    fifoSendValue32(FIFO_USER_01, 0); // Request initial jack state.
    // The application owns lid policy: headphones keep playing; otherwise
    // it stops application work, PCM transport and the DSP clock.
    disableSleep();
    systemCounterSetup();
}

int sounda_is_dsi(void) { return isDSiMode(); }
int sounda_mount_sd(void) { return fatInitDefault() ? 0 : -1; }
uint32_t sounda_ms(void) { return (uint32_t)(systemCounterGetTicks() * 64u / 33514u); }
void sounda_vblank(void) { swiWaitForVBlank(); }
uint32_t sounda_keys(void) { scanKeys(); return keysDown() | (keysDownRepeat() & (KEY_UP | KEY_DOWN | KEY_LEFT | KEY_RIGHT)); }
uint32_t sounda_held(void) { return keysHeld(); }
int sounda_headphones(void) { return (int)(headphones & 1u); }
void sounda_dsp_clock(int on)
{
    if (isDSiMode() && (REG_SCFG_EXT & SCFG_EXT_DSP)) dspSetClockEnabled(on != 0);
}
uint32_t sounda_rtc(void)
{
    time_t now = time(NULL);
    struct tm *date = localtime(&now);
    if (!date) return 0;
    return (uint32_t)(date->tm_hour * 3600 + date->tm_min * 60 + date->tm_sec);
}
uint32_t sounda_touch(void)
{
    touchPosition pos;
    touchRead(&pos);
    return ((uint32_t)pos.py << 16) | pos.px;
}
void sounda_backlights(int on)
{
    if (on) powerOn(PM_BACKLIGHT_TOP | PM_BACKLIGHT_BOTTOM);
    else powerOff(PM_BACKLIGHT_TOP | PM_BACKLIGHT_BOTTOM);
}
void sounda_present(const uint16_t *top, const uint16_t *bottom)
{
    const size_t bytes = 256u * 192u * sizeof(uint16_t);
    DC_FlushRange(top, bytes);
    DC_FlushRange(bottom, bytes);
    dmaCopyWords(3, top, bgGetGfxPtr(backgrounds[0]), bytes);
    dmaCopyWords(3, bottom, bgGetGfxPtr(backgrounds[1]), bytes);
}
void sounda_present_rows(unsigned int screen, unsigned int first,
                         unsigned int count, const uint16_t *frame)
{
    if (screen > 1 || first >= 192 || count == 0 || count > 192 - first) return;
    const uint16_t *source = frame + first * 256u;
    size_t bytes = count * 256u * sizeof(uint16_t);
    DC_FlushRange(source, bytes);
    dmaCopyWords(3, source, bgGetGfxPtr(backgrounds[screen]) + first * 256u, bytes);
}
void sounda_diagnostic(unsigned int index, uint32_t value)
{
    if (index >= 8 && index < 40) sounda_diagnostics[index] = value;
    // Emulators inspect physical RAM without snooping the ARM9 data cache.
    // Publish at most once per display frame, rather than on every DSP poll.
    static uint32_t published;
    if (index == 17 && value - published >= 16)
    {
        published = value;
        DC_FlushRange((const void *)sounda_diagnostics, sizeof(sounda_diagnostics));
    }
}
void sounda_dsp_decoded(uint32_t milliseconds)
{
    sounda_diagnostics[4]++;
    if (milliseconds > sounda_diagnostics[8]) sounda_diagnostics[8] = milliseconds;
    sounda_diagnostics[15] = REG_DSP_PSTS;
    sounda_diagnostics[16] = REG_SCFG_EXT;
}
void sounda_fatal(void)
{
    BG_PALETTE[0] = RGB15(31, 0, 4);
    BG_PALETTE_SUB[0] = RGB15(31, 0, 4);
    videoSetMode(MODE_0_2D);
    videoSetModeSub(MODE_0_2D);
}

void *sounda_file_open(const char *path, uint64_t *length)
{
    FILE *file = fopen(path, "rb");
    if (!file) return NULL;
    (void)setvbuf(file, NULL, _IOFBF, 32768);
    if (fseek(file, 0, SEEK_END) != 0) { fclose(file); return NULL; }
    long size = ftell(file);
    if (size < 0 || fseek(file, 0, SEEK_SET) != 0) { fclose(file); return NULL; }
    *length = (uint64_t)size;
    return file;
}
int sounda_file_read(void *handle, uint64_t offset, uint8_t *data, uint32_t length)
{
    FILE *file = handle;
    if (offset > INT32_MAX) return -1;
    if (ftell(file) != (long)offset && fseek(file, (long)offset, SEEK_SET) != 0) return -1;
    return fread(data, 1, length, file) == length ? 0 : -1;
}
void sounda_file_close(void *file) { (void)fclose(file); }
void *sounda_dir_open(const char *path) { return opendir(path); }
// Return 0 at EOF, 1 file, 2 directory, 3 skipped entry, -1 I/O error.
// One call consumes at most one raw entry, including hidden/unsupported ones.
int sounda_dir_next(void *handle, const char *base, char *name, uint32_t capacity)
{
    errno = 0;
    struct dirent *entry = readdir(handle);
    if (!entry) return errno == 0 ? 0 : -1;
    if (entry->d_name[0] == '.') return 3;
    size_t len = strlen(entry->d_name);
    if (len >= capacity) return 3;
    unsigned int kind = entry->d_type;
    // FatFs already returns the FAT directory attribute in d_type. Only
    // filesystems which explicitly lack that information need a stat.
    if (kind == DT_UNKNOWN)
    {
        char path[1024];
        int n = snprintf(path, sizeof(path), "%s%s%s", base,
                         base[strlen(base) - 1] == '/' ? "" : "/", entry->d_name);
        if (n < 0 || (size_t)n >= sizeof(path)) return 3;
        struct stat status;
        if (stat(path, &status) != 0) return -1;
        kind = S_ISDIR(status.st_mode) ? DT_DIR : S_ISREG(status.st_mode) ? DT_REG : DT_UNKNOWN;
    }
    if (kind != DT_DIR && kind != DT_REG) return 3;
    memcpy(name, entry->d_name, len + 1);
    return kind == DT_DIR ? 2 : 1;
}
void sounda_dir_close(void *handle) { (void)closedir(handle); }

// Own B/C only; leave shared WRAM A used by the SDK untouched.
int sounda_dsp_prepare(void)
{
    // SCFG_EXT_DSP is a power/access switch, not a persistent permission:
    // dspPowerOff() clears it. Requiring it here prevents EVERY second boot.
    if (!isDSiMode() || !nwramIsAvailable()) return -1;
    dspPowerOff();
    nwramSetBlockMapping(NWRAM_BLOCK_B, 0x03000000, 256 * 1024, NWRAM_BLOCK_IMAGE_SIZE_256K);
    nwramSetBlockMapping(NWRAM_BLOCK_C, 0x03400000, 256 * 1024, NWRAM_BLOCK_IMAGE_SIZE_256K);
    for (int i = 0; i < NWRAM_BC_SLOT_COUNT; ++i)
    {
        if (nwramMapWramBSlot(i, NWRAM_B_SLOT_MASTER_ARM9, i, true) < 0 ||
            nwramMapWramCSlot(i, NWRAM_C_SLOT_MASTER_ARM9, i, true) < 0) return -1;
    }
    memset((void *)0x03000000, 0, 256 * 1024);
    memset((void *)0x03400000, 0, 256 * 1024);
    return 0;
}
void sounda_dsp_section(int code, uint16_t address, const uint8_t *data, uint32_t size)
{
    // Rust has already validated address, length, and every source range.
    // NOLOAD regions remain zero from prepare().
    if (data) memcpy((void *)((code ? 0x03000000u : 0x03400000u) + (uint32_t)address * 2u), data, size);
}
int sounda_dsp_boot(void)
{
    for (int i = 0; i < NWRAM_BC_SLOT_COUNT; ++i)
    {
        if (nwramMapWramBSlot(i, NWRAM_B_SLOT_MASTER_DSP_CODE, i, true) < 0 ||
            nwramMapWramCSlot(i, NWRAM_C_SLOT_MASTER_DSP_DATA, i, true) < 0) return -1;
    }
    nwramSetBlockMapping(NWRAM_BLOCK_B, NWRAM_BASE, 0, NWRAM_BLOCK_IMAGE_SIZE_32K);
    nwramSetBlockMapping(NWRAM_BLOCK_C, NWRAM_BASE, 0, NWRAM_BLOCK_IMAGE_SIZE_32K);
    dspPowerOn();
    if (!(REG_SCFG_EXT & SCFG_EXT_DSP)) return -1;
    dspSetCoreResetOff(0); // Poll replies: no shared WiFi/DSP IRQ handler needed.
    dspSetSemaphoreMask(0xffff);
    sounda_diagnostics[2]++;
    sounda_diagnostics[16] = REG_SCFG_EXT;
    return 0;
}
void sounda_dsp_off(void)
{
    if (isDSiMode() && (REG_SCFG_EXT & SCFG_EXT_DSP))
    {
        dspPowerOff();
        sounda_diagnostics[3]++;
    }
}
int sounda_dsp_send(uint16_t word)
{
    if (!dspSendDataReady(1)) return 0;
    dspSendData(1, word);
    return 1;
}
int sounda_dsp_reply(unsigned int reg)
{
    if (reg > 2 || !dspReceiveDataReady((int)reg)) return -1;
    return dspReceiveData((int)reg);
}
void sounda_dsp_housekeeping(void)
{
    // The decoder doesn't use pipes, but the common firmware sends its pipe
    // table pointer through REP2 and sets semaphore 15 during initialization.
    if (dspReceiveDataReady(2)) (void)dspReceiveData(2);
    dspAckSemaphore(0xffff);
}

static uint8_t compressed[1728] __attribute__((aligned(32)));
static int16_t decoded_left[1024] __attribute__((aligned(32)));
static int16_t decoded_right[1024] __attribute__((aligned(32)));

void sounda_dsp_buffers(const uint8_t *frame, uint32_t length, uint32_t *addresses)
{
    memset(compressed, 0, sizeof(compressed));
    memcpy(compressed, frame, length);
    DC_FlushRange(compressed, sizeof(compressed));
    DC_FlushRange(decoded_left, sizeof(decoded_left));
    DC_FlushRange(decoded_right, sizeof(decoded_right));
    addresses[0] = (uint32_t)compressed;
    addresses[1] = (uint32_t)decoded_left;
    addresses[2] = (uint32_t)decoded_right;
}

#endif // SOUNDA_AUDIO_TEST

// PCM transport. Maxmod receives only already-decoded interleaved PCM.
// Timer 1 refills the Maxmod stream independently of rendering and SD I/O.
// Maxmod's manual timer 0 clock is retained: mmStreamGetPosition() is NOT
// implemented in its automatic mode in this SDK. No SD, DSP, allocation or
// Rust code runs in our interrupt handler.
#define PCM_CAPACITY 65536u
#define PCM_MASK (PCM_CAPACITY - 1u)
#define SEGMENTS 128u
static int16_t pcm[PCM_CAPACITY * 2] __attribute__((aligned(32)));
static volatile uint32_t produced, submitted, played;
static uint32_t timeline;
static uint32_t stream_rate;
static volatile bool stream_active, stream_paused, stream_eof;
static bool stream_started;
static volatile uint32_t underruns;
typedef struct { uint32_t clock, source, length, data; } Segment;
static Segment ledger[SEGMENTS];
static unsigned int ledger_head, ledger_count;

static void reconcile_position_unlocked(void)
{
    if (!stream_active) return;
    uint32_t position = mmStreamGetPosition();
    while (ledger_count)
    {
        Segment *s = &ledger[ledger_head];
        if (position < s->clock) break;
        uint32_t advance = position - s->clock;
        if (advance > s->data) advance = s->data;
        played = s->source + advance;
        if (position - s->clock < s->length) break;
        ledger_head = (ledger_head + 1u) % SEGMENTS;
        ledger_count--;
    }
    sounda_diagnostics[6] = produced - played;
    if (sounda_diagnostics[6] < sounda_diagnostics[7])
        sounda_diagnostics[7] = sounda_diagnostics[6];
}
static void reconcile_position(void)
{
    int previous = enterCriticalSection();
    __asm__ volatile ("" ::: "memory");
    reconcile_position_unlocked();
    __asm__ volatile ("" ::: "memory");
    leaveCriticalSection(previous);
}

static mm_word pcm_callback(mm_word length, mm_addr destination, mm_stream_formats format)
{
    (void)format;
    reconcile_position_unlocked();
    int16_t *dst = destination;
    uint32_t available = produced - submitted;
    uint32_t take = length < available ? length : available;
    // At most 8192 output samples can be in flight, but keep an explicit
    // bound so a faulty transport never overwrites unplayed data.
    if (ledger_count == SEGMENTS) take = 0;
    bool merged = false;
    if (ledger_count && take == length)
    {
        Segment *last = &ledger[(ledger_head + ledger_count - 1u) % SEGMENTS];
        if (last->data == last->length && last->clock + last->length == timeline &&
            last->source + last->data == submitted)
        {
            last->length += length;
            last->data += length;
            merged = true;
        }
    }
    if (!merged && ledger_count < SEGMENTS)
    {
        unsigned int index = (ledger_head + ledger_count) % SEGMENTS;
        ledger[index] = (Segment){ timeline, submitted, length, take };
        ledger_count++;
    }
    for (uint32_t i = 0; i < take; ++i)
    {
        uint32_t index = ((submitted + i) & PCM_MASK) * 2u;
        *dst++ = pcm[index];
        *dst++ = pcm[index + 1u];
    }
    memset(dst, 0, (length - take) * 4u);
    if (take < length && !stream_eof) underruns++;
    sounda_diagnostics[5] = underruns;
    sounda_diagnostics[18]++;
    if (ledger_count > sounda_diagnostics[19]) sounda_diagnostics[19] = ledger_count;
    submitted += take;
    timeline += length;
    return length;
}

static void stream_interrupt(void)
{
    if (!stream_active || stream_paused) return;
    mmStreamUpdate();
    reconcile_position_unlocked();
}

int sounda_audio_init(void)
{
    if (!mmInitNoSoundbank()) return -1;
    soundEnable();
    soundSetMasterVolume(127);
    soundExtSetFrequency(47);
    soundExtSetRatio(8); // PCM plays on ARM7 channels: use their full output.
    return 0;
}
void sounda_audio_reset(uint32_t rate)
{
    timerStop(1);
    if (stream_active) mmStreamClose();
    stream_active = false;
    stream_paused = false;
    stream_eof = false;
    stream_started = false;
    produced = submitted = played = timeline = 0;
    ledger_head = ledger_count = 0;
    underruns = 0;
    stream_rate = rate;
    sounda_diagnostics[5] = sounda_diagnostics[6] = 0;
    sounda_diagnostics[7] = UINT32_MAX;
}
uint32_t sounda_audio_free(void) { reconcile_position(); return PCM_CAPACITY - (produced - played); }
uint32_t sounda_audio_queued(void) { reconcile_position(); return produced - played; }
uint32_t sounda_audio_played(void) { reconcile_position(); return played; }
uint32_t sounda_audio_underruns(void) { return underruns; }
int sounda_audio_active(void) { return stream_active && !stream_paused; }
int sounda_audio_push(unsigned int channels)
{
    if (sounda_audio_free() < 1024) return -1;
    DC_InvalidateRange(decoded_left, sizeof(decoded_left));
    DC_InvalidateRange(decoded_right, sizeof(decoded_right));
    for (uint32_t i = 0; i < 1024; ++i)
    {
        uint32_t index = ((produced + i) & PCM_MASK) * 2u;
        pcm[index] = decoded_left[i];
        pcm[index + 1u] = channels == 1 ? decoded_left[i] : decoded_right[i];
    }
    // Publish only complete frames to the interrupt consumer.
    __asm__ volatile ("" ::: "memory");
    produced += 1024;
    return 0;
}
static void start_stream(void)
{
    ledger_head = ledger_count = 0;
    timeline = 0;
    submitted = played;
    mm_stream stream = {
        .sampling_rate = stream_rate,
        .buffer_length = 8192,
        .callback = pcm_callback,
        .format = MM_STREAM_16BIT_STEREO,
        .timer = MM_TIMER0,
        .manual = true,
    };
    mmStreamOpen(&stream);
    mmStreamVolume(127);
    soundSetMasterVolume(127);
    stream_active = true;
    stream_started = true;
    timerStart(1, ClockDivider_1024, (uint16_t)TIMER_FREQ_1024(120), stream_interrupt);
}
void sounda_audio_service(void)
{
    if (stream_paused) return;
    if (!stream_active && (produced - played >= 16384 || (stream_eof && produced > played))) start_stream();
    if (stream_active) reconcile_position();
}
void sounda_audio_pause(int pause)
{
    if (pause && stream_active)
    {
        timerStop(1);
        reconcile_position();
        mmStreamClose();
        stream_active = false;
        // Retain the submitted-but-unplayed tail so pause never skips audio.
        submitted = played;
        ledger_head = ledger_count = 0;
    }
    stream_paused = pause != 0;
    // Resuming a real stream retains its unplayed tail. A pause during the
    // initial buffer fill must not bypass the normal prebuffer threshold.
    if (!pause && stream_started && produced > played && !stream_active) start_stream();
}
void sounda_audio_eof(void) { stream_eof = true; }
int sounda_audio_finished(void)
{
    reconcile_position();
    return stream_eof && played == produced;
}
