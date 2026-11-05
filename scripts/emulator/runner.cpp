// SPDX-License-Identifier: GPL-3.0-or-later
#include "Args.h"
#include "DSi.h"
#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <vector>

using namespace melonDS;
namespace melonDS::Platform { extern bool AllowHostAAC; extern unsigned HostAACCalls; }
namespace fs = std::filesystem;
template<size_t N> std::unique_ptr<std::array<u8, N>> load(const fs::path& p) {
    auto b = std::make_unique<std::array<u8, N>>();
    std::ifstream f(p, std::ios::binary);
    if (!f.read(reinterpret_cast<char*>(b->data()), N)) throw std::runtime_error("Invalid BIOS: " + p.string());
    return b;
}
std::vector<u8> bytes(const fs::path& p) {
    std::ifstream f(p, std::ios::binary | std::ios::ate);
    if (!f) throw std::runtime_error("Cannot open " + p.string());
    size_t n = f.tellg(); f.seekg(0); std::vector<u8> b(n);
    if (!f.read(reinterpret_cast<char*>(b.data()), n)) throw std::runtime_error("Cannot read " + p.string());
    return b;
}
void snapshot(DSi& d, const fs::path& out, int frame) {
    std::ostringstream name; name << "frame-" << std::setw(5) << std::setfill('0') << frame << ".ppm";
    std::ofstream f(out / name.str(), std::ios::binary); f << "P6\n256 384\n255\n";
    for (int screen = 0; screen < 2; ++screen) {
        auto* p = d.GPU.Framebuffer[d.GPU.FrontBuffer][screen].get();
        if (!p) throw std::runtime_error("No framebuffer");
        for (int i = 0; i < 256 * 192; ++i) {
            unsigned v = p[i];
            char rgb[3] = {char((v >> 16) & 255), char((v >> 8) & 255), char(v & 255)};
            f.write(rgb, 3);
        }
    }
}
struct Event { int frame; std::string kind; int a = 0, b = 0; };
int main(int argc, char** argv) try {
    if (argc < 6) {
        std::cerr << "Usage: sounda-emulator ROM BIOS_DIR SD_FOLDER OUTPUT_DIR FRAMES [EVENTS_TSV] [GDB_ARM9_PORT] [DIAGNOSTIC_ADDRESS] [lle|hle]\n";
        return 2;
    }
    fs::path rom = fs::absolute(argv[1]), bios = fs::absolute(argv[2]), sd = fs::absolute(argv[3]), out = fs::absolute(argv[4]);
    int frames = std::stoi(argv[5]); fs::create_directories(out);
    auto nandpath = out / "nand-copy.bin";
    fs::copy_file(bios / "dsinand.bin", nandpath, fs::copy_options::overwrite_existing);
    NDSArgs n;
    n.ARM9BIOS = load<ARM9BIOSSize>(bios / "biosnds9.bin");
    n.ARM7BIOS = load<ARM7BIOSSize>(bios / "biosnds7.bin");
    auto fw = bytes(bios / "dsifirmware.bin"); n.Firmware = Firmware(fw.data(), fw.size());
    n.JIT = std::nullopt; n.OutputSampleRate = 48000;
    if (argc > 7 && std::stoi(argv[7])) {
        auto port = static_cast<u16>(std::stoi(argv[7]));
        n.GDB = GDBArgs{static_cast<u16>(port + 1), port, false, std::getenv("SOUNDA_GDB_BREAK") != nullptr};
    }
    auto bi9 = load<DSiBIOSSize>(bios / "biosdsi9.bin");
    auto bi7 = load<DSiBIOSSize>(bios / "biosdsi7.bin");
    auto nf = Platform::OpenFile(nandpath.string(), Platform::ReadWriteExisting);
    if (!nf) throw std::runtime_error("Cannot open isolated NAND copy");
    DSi_NAND::NANDImage nand(nf, &(*bi7)[0x8308]);
    if (!nand) throw std::runtime_error("Invalid NAND dump");
    FATStorage sdcard((out / "sd.bin").string(), 256ULL * 1024 * 1024, false, sd.string());
    bool hle = argc > 9 && std::string(argv[9]) == "hle";
    Platform::AllowHostAAC = hle;
    DSiArgs args{std::move(n), std::move(bi9), std::move(bi7), std::move(nand), std::move(sdcard), false, hle};
    auto console = std::make_unique<DSi>(std::move(args));
    auto& d = *console;
    auto rb = bytes(rom); auto cart = NDSCart::ParseROM(rb.data(), rb.size());
    if (!cart) throw std::runtime_error("Cannot parse ROM");
    d.SetNDSCart(std::move(cart)); d.Reset();
    d.RTC.SetDateTime(2026, 10, 15, 12, 0, 0);
    d.NDS::SetupDirectBoot(rom.filename().string()); d.Start();
    std::vector<Event> events;
    if (argc > 6) {
        std::ifstream ef(argv[6]); std::string line;
        while (std::getline(ef, line)) {
            if (line.empty() || line[0] == '#') continue;
            std::istringstream is(line); Event e;
            if (is >> e.frame >> e.kind) { is >> e.a >> e.b; events.push_back(e); }
        }
        std::stable_sort(events.begin(), events.end(), [](auto& a, auto& b){ return a.frame < b.frame; });
    }
    std::ofstream audio(out / "audio.s16le", std::ios::binary);
    std::ofstream trace(out / "trace.csv");
    trace << "frame,wall_us,audio_frames,nonzero,peak,arm9_pc,arm7_pc,scfg_clock9,scfg_ext9,scfg_rst,dsp_pcfg,dsp_psts,list_hash\n";
    size_t eventindex = 0; u32 keys = 0xfff; s16 pcm[8192];
    std::string failure;
    unsigned diagnostic_address = argc > 8 ? std::stoul(argv[8], nullptr, 0) : 0;
    std::ofstream diagnostics(out / "diagnostics.csv");
    if (diagnostic_address) {
        diagnostics << "frame";
        for (int i = 0; i < 40; ++i) diagnostics << ",word" << i;
        diagnostics << '\n';
    }
    u64 total = 0, nonzero = 0; auto start = std::chrono::steady_clock::now();
    for (int frame = 0; frame < frames && d.IsRunning(); ++frame) {
        while (eventindex < events.size() && events[eventindex].frame == frame) {
            auto& e = events[eventindex++];
            if (e.kind == "keys") keys = 0xfff ^ e.a;
            else if (e.kind == "touch") d.TouchScreen(e.a, e.b);
            else if (e.kind == "release") d.ReleaseScreen();
            else if (e.kind == "lid") d.SetLidClosed(e.a != 0);
            else if (e.kind == "headphones") {
                if (e.a) d.GPIO_Data |= 0x20; else d.GPIO_Data &= ~0x20;
            }
            else if (e.kind == "shot") snapshot(d, out, frame);
            std::cerr << "EVENT " << frame << " " << e.kind << " " << e.a << " " << e.b << "\n";
        }
        d.SetKeyMask(keys);
        try { d.RunFrame(); }
        catch (const std::exception& e) {
            failure = e.what();
            std::cerr << "CORE_EXCEPTION frame=" << frame << " " << failure << '\n';
            snapshot(d, out, frame);
            break;
        }
        int got = d.SPU.ReadOutput(pcm, 4096), nz = 0, peak = 0;
        audio.write(reinterpret_cast<char*>(pcm), got * 2 * sizeof(*pcm));
        for (int i = 0; i < got * 2; ++i) { nz += pcm[i] != 0; peak = std::max(peak, std::abs(int(pcm[i]))); }
        total += got; nonzero += nz;
        auto us = std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now() - start).count();
        auto* fb = d.GPU.Framebuffer[d.GPU.FrontBuffer][1].get();
        unsigned list_hash = fb ? CRC32(reinterpret_cast<const u8*>(fb + 35 * 256), 18 * 256 * 4) : 0;
        trace << frame << ',' << us << ',' << got << ',' << nz << ',' << peak << ','
              << d.ARM9.R[15] << ',' << d.ARM7.R[15] << ',' << d.SCFG_Clock9 << ',' << d.SCFG_EXT[0] << ',' << d.SCFG_RST << ','
              << d.DSP.Read16(0x04004308) << ',' << d.DSP.Read16(0x0400430c) << ',' << list_hash << '\n';
        if (diagnostic_address) {
            diagnostics << frame;
            for (int i = 0; i < 40; ++i) diagnostics << ',' << d.ARM9Read32(diagnostic_address + i * 4);
            diagnostics << '\n';
        }
        if (frame % 60 == 0 || frame + 1 == frames) { snapshot(d, out, frame); trace.flush(); audio.flush(); }
    }
    std::ofstream ram(out / "arm9-mainram.bin", std::ios::binary);
    ram.write(reinterpret_cast<char*>(d.MainRAM), d.MainRAMMaxSize);
    std::ofstream itcm(out / "arm9-itcm.bin", std::ios::binary);
    itcm.write(reinterpret_cast<char*>(d.ARM9.ITCM), 32768);
    std::ofstream code(out / "nwram-b.bin", std::ios::binary);
    code.write(reinterpret_cast<char*>(d.NWRAM_B), d.NWRAMSize);
    std::ofstream data(out / "nwram-c.bin", std::ios::binary);
    data.write(reinterpret_cast<char*>(d.NWRAM_C), d.NWRAMSize);
    std::ofstream report(out / "runner.json");
    report << "{\"emulator\":\"melonDS 1.1\",\"source_commit\":\"b86390e4428bf38ce4c1ce0e9ca446d6d25955e8\",\"dsp\":\"" << (hle ? "AAC HLE/FAAD" : "Teakra LLE") << "\",\"hle_aac_decoder\":" << (hle ? "true" : "false") << ",\"host_aac_decode_calls\":" << Platform::HostAACCalls << ",\"core_exception\":" << (failure.empty() ? "false" : "true") << ",\"audio_sample_rate\":48000,\"audio_frames\":" << total << ",\"nonzero_samples\":" << nonzero << ",\"running\":" << (d.IsRunning() ? "true" : "false") << "}\n";
    std::cout << "Captured " << total << " stereo PCM frames, " << nonzero << " nonzero samples; DSP " << (hle ? "HLE" : "LLE") << ".\n";
    return failure.empty() ? 0 : 1;
} catch (const std::exception& e) { std::cerr << "ERROR " << e.what() << '\n'; return 1; }
