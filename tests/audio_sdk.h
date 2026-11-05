// Host-only declarations for testing the real C queue with a simulated clock.
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <assert.h>

typedef uint32_t mm_word;
typedef void *mm_addr;
typedef enum { MM_STREAM_16BIT_STEREO = 3 } mm_stream_formats;
typedef mm_word (*mm_stream_func)(mm_word, mm_addr, mm_stream_formats);
typedef struct {
    mm_word sampling_rate, buffer_length;
    mm_stream_func callback;
    mm_word format, timer;
    bool manual;
} mm_stream;
#define MM_TIMER0 0
#define ClockDivider_1024 3
#define TIMER_FREQ_1024(n) (-(32728 / (n)))
void timerStart(int timer, unsigned int divider, uint16_t ticks, void (*callback)(void));
uint16_t timerStop(int timer);
static inline int enterCriticalSection(void) { return 1; }
static inline void leaveCriticalSection(int previous) { (void)previous; }
bool mmInitNoSoundbank(void);
void mmStreamOpen(mm_stream *stream);
void mmStreamClose(void);
void mmStreamVolume(unsigned int volume);
void mmStreamUpdate(void);
uint32_t mmStreamGetPosition(void);
void soundEnable(void);
void soundSetMasterVolume(unsigned int volume);
void soundExtSetFrequency(unsigned int frequency);
void soundExtSetRatio(unsigned int ratio);
static inline void DC_InvalidateRange(const void *data, unsigned int length) { (void)data; (void)length; }
