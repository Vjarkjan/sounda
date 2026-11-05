// This compiles the actual production transport. It does not emulate the DSP,
// ARM7 FIFO, sound channel rate divider, cache or DAC.
#define SOUNDA_AUDIO_TEST
#include "../native/nds.c"
#include <stdio.h>

static mm_stream_func mock_callback;
static uint32_t mock_clock, mock_updated;
static bool mock_active;
static void (*mock_interrupt)(void);
static int16_t mock_output[16384];
static uint32_t mock_length;
static unsigned int master_volume, channel_volume;

bool mmInitNoSoundbank(void) { return true; }
void soundEnable(void) {}
void soundSetMasterVolume(unsigned int volume) { master_volume = volume; }
void soundExtSetFrequency(unsigned int frequency) { assert(frequency == 47); }
void soundExtSetRatio(unsigned int ratio) { assert(ratio == 8); }
void mmStreamVolume(unsigned int volume) { channel_volume = volume; }
uint32_t mmStreamGetPosition(void) { assert(mock_active); return mock_clock; }
void mmStreamClose(void) { assert(mock_active); mock_active = false; }
void timerStart(int timer, unsigned int divider, uint16_t ticks, void (*callback)(void))
{
    assert(timer == 1 && divider == ClockDivider_1024 && ticks != 0);
    mock_interrupt = callback;
}
uint16_t timerStop(int timer)
{
    assert(timer == 1);
    mock_interrupt = NULL;
    return 0;
}
static void request(uint32_t length)
{
    assert(length <= 8192);
    mock_length = length;
    assert(mock_callback(length, mock_output, MM_STREAM_16BIT_STEREO) == length);
}
void mmStreamOpen(mm_stream *stream)
{
    assert(!mock_active);
    assert(stream->manual && stream->timer == MM_TIMER0);
    assert(stream->format == MM_STREAM_16BIT_STEREO);
    mock_callback = stream->callback;
    mock_clock = mock_updated = 0;
    mock_active = true;
    // Maxmod prefills length - 16 frames before it starts its sample clock.
    request(stream->buffer_length - 16u);
}
void mmStreamUpdate(void)
{
    assert(mock_active);
    uint32_t amount = mock_clock - mock_updated;
    mock_updated = mock_clock;
    if (amount) request(amount);
}
static void tick(uint32_t samples)
{
    while (samples)
    {
        uint32_t step = samples > 512 ? 512 : samples;
        mock_clock += step;
        // A stalled main/UI/SD task does not call sounda_audio_service().
        assert(mock_interrupt);
        mock_interrupt();
        samples -= step;
    }
}
static void push(unsigned int channels, int16_t left, int16_t right)
{
    for (unsigned int i = 0; i < 1024; ++i) { decoded_left[i] = left; decoded_right[i] = right; }
    assert(sounda_audio_push(channels) == 0);
}

static void test_pause_and_stereo(void)
{
    sounda_audio_reset(44100);
    for (unsigned int i = 0; i < 16; ++i) push(2, (int16_t)(1000u + i), (int16_t)(2000u + i));
    assert(sounda_audio_free() == PCM_CAPACITY - 16384);
    sounda_audio_service();
    assert(sounda_audio_active());
    assert(mock_output[0] == 1000 && mock_output[1] == 2000);
    tick(1500);
    assert(sounda_audio_played() == 1500);
    sounda_audio_pause(1);
    assert(!mock_active && mock_interrupt == NULL && submitted == 1500 && produced == 16384);
    sounda_audio_pause(0);
    assert(mock_output[0] == 1001 && mock_output[1] == 2001);
    tick(100);
    assert(sounda_audio_played() == 1600);
    assert(master_volume == 127 && channel_volume == 127);
    sounda_audio_reset(44100);
    puts("PASS pause retains the unplayed stereo tail and maximum software volume");
}
static void test_short_mono_eof(void)
{
    sounda_audio_reset(8000);
    push(1, 1234, -999);
    sounda_audio_eof();
    sounda_audio_service();
    assert(mock_length == 8176 && mock_output[0] == 1234 && mock_output[1] == 1234);
    assert(mock_output[2048] == 0 && mock_output[2049] == 0);
    assert(sounda_audio_underruns() == 0);
    tick(1023); assert(!sounda_audio_finished());
    tick(1); assert(sounda_audio_finished());
    sounda_audio_reset(44100);
    puts("PASS short mono EOF drains exactly once and silence does not delay completion");
}
static void test_queue_capacity_and_wrap(void)
{
    sounda_audio_reset(48000);
    for (unsigned int i = 0; i < 64; ++i) push(2, 42, -42);
    assert(sounda_audio_free() == 0);
    assert(sounda_audio_push(2) == -1);
    sounda_audio_service();
    for (unsigned int i = 0; i < 80; ++i)
    {
        tick(1024);
        assert(sounda_audio_free() >= 1024);
        push(2, (int16_t)i, -(int16_t)i);
        assert(sounda_audio_queued() == PCM_CAPACITY);
    }
    assert(sounda_audio_underruns() == 0);
    sounda_audio_reset(44100);
    puts("PASS full queue rejects overflow and wraps without losing accounting");
}
static void test_underrun_pause(void)
{
    sounda_audio_reset(44100);
    for (unsigned int i = 0; i < 16; ++i) push(2, 12, -12);
    sounda_audio_service();
    tick(24576);
    assert(sounda_audio_played() == 16384 && sounda_audio_underruns() > 0);
    // New samples are submitted after an audible silence interval. They must
    // not be counted as played just because the hardware clock kept running.
    for (unsigned int i = 0; i < 4; ++i) push(2, 987, -987);
    tick(128);
    assert(submitted > played && played == 16384);
    sounda_audio_pause(1);
    sounda_audio_pause(0);
    assert(mock_output[0] == 987 && mock_output[1] == -987);
    sounda_audio_eof();
    tick(4096);
    assert(sounda_audio_played() == 20480 && sounda_audio_finished());
    sounda_audio_reset(44100);
    puts("PASS underrun silence is excluded from played data and pause preserves later samples");
}
static void test_interrupt_refill_and_tiny_callbacks(void)
{
    sounda_audio_reset(44100);
    for (unsigned int i = 0; i < 64; ++i) push(2, 987, -987);
    sounda_audio_service();
    // Reproduce the previous ledger pressure: thousands of tiny callbacks.
    // Previously 128 entries filled before the hardware consumed the prefill,
    // so available PCM was replaced with silence.
    for (unsigned int i = 0; i < 4096; ++i)
    {
        tick(4);
        assert(mock_output[0] == 987 && mock_output[1] == -987);
    }
    assert(sounda_audio_played() == 16384);
    assert(sounda_audio_underruns() == 0 && ledger_count <= 2);
    // No main-loop service for another half second; IRQ still drains PCM.
    tick(22050);
    assert(sounda_audio_played() == 38434 && sounda_audio_underruns() == 0);
    sounda_audio_reset(44100);
    assert(mock_interrupt == NULL);
    puts("PASS IRQ refill survives a stalled main loop and tiny callbacks cannot exhaust the ledger");
}
static void test_pause_during_initial_buffering(void)
{
    sounda_audio_reset(44100);
    push(2, 543, -543);
    sounda_audio_service();
    assert(!sounda_audio_active());
    sounda_audio_pause(1);
    sounda_audio_pause(0);
    assert(!sounda_audio_active() && mock_interrupt == NULL);
    assert(sounda_audio_underruns() == 0);
    for (unsigned int i = 1; i < 16; ++i) push(2, 543, -543);
    sounda_audio_service();
    assert(sounda_audio_active() && mock_output[0] == 543);
    assert(sounda_audio_underruns() == 0);
    sounda_audio_reset(44100);
    puts("PASS pause during initial buffering preserves the startup threshold without inserting silence");
}
int main(void)
{
    assert(sounda_audio_init() == 0);
    test_pause_and_stereo();
    test_short_mono_eof();
    test_queue_capacity_and_wrap();
    test_underrun_pause();
    test_interrupt_refill_and_tiny_callbacks();
    test_pause_during_initial_buffering();
    return 0;
}
