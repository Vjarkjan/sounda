// SPDX-License-Identifier: GPL-3.0-or-later
// Minimal isolated frontend for the official melonDS core. No user settings,
// networking, save files, microphone or camera are used. The optional HLE AAC
// path is explicitly selected, counted and labelled separately from DSP LLE.
#include "Platform.h"
#include <chrono>
#include <condition_variable>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <filesystem>
#include <mutex>
#include <thread>

namespace melonDS::Platform {
struct FileHandle { FILE* stream; };
void SignalStop(StopReason reason, void*) { std::fprintf(stderr, "EMU_STOP %d\n", int(reason)); }
std::string GetLocalFilePath(const std::string& s) { return s; }
FileHandle* OpenFile(const std::string& s, FileMode m) {
    bool exists = std::filesystem::exists(s);
    if ((m & NoCreate) && !exists) return nullptr;
    bool write = m & Write;
    const char* mode = !write ? "rb" : ((m & Append) ? "ab+" :
        (((m & Preserve) && exists) || (m & NoCreate)) ? "rb+" : "wb+");
    FILE* f = std::fopen(s.c_str(), mode);
    return f ? new FileHandle{f} : nullptr;
}
FileHandle* OpenLocalFile(const std::string& s, FileMode m) { return OpenFile(s, m); }
bool FileExists(const std::string& s) { return std::filesystem::exists(s); }
bool LocalFileExists(const std::string& s) { return FileExists(s); }
bool CheckFileWritable(const std::string& s) {
    auto* f = OpenFile(s, FileMode(Write | Preserve));
    return f ? CloseFile(f) : false;
}
bool CheckLocalFileWritable(const std::string& s) { return CheckFileWritable(s); }
bool CloseFile(FileHandle* f) { int r = std::fclose(f->stream); delete f; return r == 0; }
bool IsEndOfFile(FileHandle* f) { return std::feof(f->stream); }
bool FileReadLine(char* s, int n, FileHandle* f) { return std::fgets(s, n, f->stream); }
u64 FilePosition(FileHandle* f) { return std::ftell(f->stream); }
bool FileSeek(FileHandle* f, s64 o, FileSeekOrigin origin) {
    int whence = origin == FileSeekOrigin::Start ? SEEK_SET : origin == FileSeekOrigin::Current ? SEEK_CUR : SEEK_END;
    return std::fseek(f->stream, o, whence) == 0;
}
void FileRewind(FileHandle* f) { std::rewind(f->stream); }
u64 FileRead(void* p, u64 s, u64 n, FileHandle* f) { return std::fread(p, s, n, f->stream); }
bool FileFlush(FileHandle* f) { return std::fflush(f->stream) == 0; }
u64 FileWrite(const void* p, u64 s, u64 n, FileHandle* f) { return std::fwrite(p, s, n, f->stream); }
u64 FileWriteFormatted(FileHandle* f, const char* fmt, ...) {
    va_list ap; va_start(ap, fmt); int n = std::vfprintf(f->stream, fmt, ap); va_end(ap); return n < 0 ? 0 : n;
}
u64 FileLength(FileHandle* f) {
    auto old = std::ftell(f->stream); std::fseek(f->stream, 0, SEEK_END);
    auto n = std::ftell(f->stream); std::fseek(f->stream, old, SEEK_SET); return n;
}
void Log(LogLevel level, const char* fmt, ...) {
    std::fprintf(stderr, "[melonDS:%d] ", int(level));
    va_list ap; va_start(ap, fmt); std::vfprintf(stderr, fmt, ap); va_end(ap);
}
struct Thread { std::thread thread; };
Thread* Thread_Create(std::function<void()> f) { return new Thread{std::thread(std::move(f))}; }
void Thread_Wait(Thread* t) { if (t->thread.joinable()) t->thread.join(); }
void Thread_Free(Thread* t) { Thread_Wait(t); delete t; }
struct Semaphore { std::mutex mutex; std::condition_variable cv; int count = 0; };
Semaphore* Semaphore_Create() { return new Semaphore; }
void Semaphore_Free(Semaphore* s) { delete s; }
void Semaphore_Reset(Semaphore* s) { std::lock_guard<std::mutex> l(s->mutex); s->count = 0; }
void Semaphore_Wait(Semaphore* s) {
    std::unique_lock<std::mutex> l(s->mutex); s->cv.wait(l, [&]{ return s->count > 0; }); --s->count;
}
bool Semaphore_TryWait(Semaphore* s, int ms) {
    std::unique_lock<std::mutex> l(s->mutex);
    if (!s->cv.wait_for(l, std::chrono::milliseconds(ms), [&]{ return s->count > 0; })) return false;
    --s->count; return true;
}
void Semaphore_Post(Semaphore* s, int n) { std::lock_guard<std::mutex> l(s->mutex); s->count += n; s->cv.notify_all(); }
struct Mutex { std::mutex mutex; };
Mutex* Mutex_Create() { return new Mutex; }
void Mutex_Free(Mutex* m) { delete m; }
void Mutex_Lock(Mutex* m) { m->mutex.lock(); }
void Mutex_Unlock(Mutex* m) { m->mutex.unlock(); }
bool Mutex_TryLock(Mutex* m) { return m->mutex.try_lock(); }
void Sleep(u64 us) { std::this_thread::sleep_for(std::chrono::microseconds(us)); }
u64 GetUSCount() { return std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now().time_since_epoch()).count(); }
u64 GetMSCount() { return GetUSCount() / 1000; }
void WriteNDSSave(const u8*, u32, u32, u32, void*) {}
void WriteGBASave(const u8*, u32, u32, u32, void*) {}
void WriteFirmware(const Firmware&, u32, u32, void*) {}
void WriteDateTime(int, int, int, int, int, int, void*) {}
void MP_Begin(void*) {}
void MP_End(void*) {}
int MP_SendPacket(u8*, int, u64, void*) { return 0; }
int MP_RecvPacket(u8*, u64*, void*) { return 0; }
int MP_SendCmd(u8*, int, u64, void*) { return 0; }
int MP_SendReply(u8*, int, u64, u16, void*) { return 0; }
int MP_SendAck(u8*, int, u64, void*) { return 0; }
int MP_RecvHostPacket(u8*, u64*, void*) { return 0; }
u16 MP_RecvReplies(u8*, u64, u16, void*) { return 0; }
int Net_SendPacket(u8*, int, void*) { return 0; }
int Net_RecvPacket(u8*, void*) { return 0; }
void Camera_Start(int, void*) {}
void Camera_Stop(int, void*) {}
void Camera_CaptureFrame(int, u32* p, int w, int h, bool, void*) { std::memset(p, 0, size_t(w) * h * 2); }
void Mic_Start(void*) {}
void Mic_Stop(void*) {}
int Mic_ReadInput(s16* p, int n, void*) { std::memset(p, 0, n * sizeof(*p)); return n; }
// LLE tests must execute the embedded Teak firmware. A call to HLE AAC decoding
// is an error and cannot silently become a host-decoder pass.
extern AACDecoder* HostAAC_Init();
extern void HostAAC_DeInit(AACDecoder*);
extern bool HostAAC_Configure(AACDecoder*, int, int);
extern bool HostAAC_DecodeFrame(AACDecoder*, const void*, int, void*, int);
bool AllowHostAAC = false;
unsigned HostAACCalls = 0;
AACDecoder* AAC_Init() {
    if (!AllowHostAAC) { Log(Error, "HLE AAC is disabled in this LLE run\n"); return nullptr; }
    return HostAAC_Init();
}
void AAC_DeInit(AACDecoder* d) { HostAAC_DeInit(d); }
bool AAC_Configure(AACDecoder* d, int f, int c) { return AllowHostAAC && HostAAC_Configure(d, f, c); }
bool AAC_DecodeFrame(AACDecoder* d, const void* i, int n, void* o, int z) {
    ++HostAACCalls;
    return AllowHostAAC && HostAAC_DecodeFrame(d, i, n, o, z);
}
bool Addon_KeyDown(KeyType, void*) { return false; }
void Addon_RumbleStart(u32, void*) {}
void Addon_RumbleStop(void*) {}
float Addon_MotionQuery(MotionQueryType, void*) { return 0; }
struct DynamicLibrary { void* handle; };
DynamicLibrary* DynamicLibrary_Load(const char* s) { auto h = dlopen(s, RTLD_LAZY); return h ? new DynamicLibrary{h} : nullptr; }
void DynamicLibrary_Unload(DynamicLibrary* l) { dlclose(l->handle); delete l; }
void* DynamicLibrary_LoadFunction(DynamicLibrary* l, const char* s) { return dlsym(l->handle, s); }
}
