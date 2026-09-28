// Independent LibRaw reference developer for bounded RAW color qualification.
//
// Build this against the pinned LibRaw 0.22.2 headers and the installed Omuse
// preview library. It deliberately uses LibRaw's C++ processing pipeline and
// camera white balance directly; it does not reproduce Omuse's FFI multiplier
// handling.

#include <libraw/libraw.h>

#include <chrono>
#include <cstdint>
#include <filesystem>
#include <iomanip>
#include <iostream>
#include <string>

namespace {
constexpr std::uint64_t kMaximumPixels = 12'500'000;

int fail(const std::string &operation, int code) {
  std::cerr << operation << ": " << LibRaw::strerror(code) << " (" << code
            << ")\n";
  return 1;
}
} // namespace

int main(int argc, char **argv) {
  if (argc != 3) {
    std::cerr << "usage: raw-photo-reference INPUT_RAW FRESH_OUTPUT.ppm\n";
    return 2;
  }

  const std::filesystem::path input(argv[1]);
  const std::filesystem::path output(argv[2]);
  std::error_code error;
  if (!std::filesystem::is_regular_file(input, error) || error) {
    std::cerr << "input must be a regular file\n";
    return 2;
  }
  if (std::filesystem::exists(output, error) || error) {
    std::cerr << "output must not already exist\n";
    return 2;
  }
  if (!output.parent_path().empty() &&
      !std::filesystem::is_directory(output.parent_path(), error)) {
    std::cerr << "output parent must already exist\n";
    return 2;
  }

  const auto started = std::chrono::steady_clock::now();
  LibRaw raw;
  raw.imgdata.params.use_camera_wb = 1;
  raw.imgdata.params.no_auto_bright = 1;
  raw.imgdata.params.output_color = 1; // sRGB
  raw.imgdata.params.output_bps = 16;
  raw.imgdata.params.bright = 1.0F;
  raw.imgdata.params.gamm[0] = static_cast<float>(1.0F - 0.55F);
  raw.imgdata.params.gamm[1] = static_cast<float>(1.0F + 3.5F);

  int code = raw.open_file(input.c_str());
  if (code != LIBRAW_SUCCESS) {
    return fail("open_file", code);
  }

  const auto &sizes = raw.imgdata.sizes;
  const std::uint64_t raw_pixels =
      std::uint64_t(sizes.raw_width) * std::uint64_t(sizes.raw_height);
  const std::uint64_t image_pixels =
      std::uint64_t(sizes.width) * std::uint64_t(sizes.height);
  if (raw_pixels == 0 || image_pixels == 0 || raw_pixels > kMaximumPixels ||
      image_pixels > kMaximumPixels) {
    std::cerr << "RAW exceeds the 12.5 MP reference-development limit: raw="
              << sizes.raw_width << 'x' << sizes.raw_height << ", image="
              << sizes.width << 'x' << sizes.height << "\n";
    return 2;
  }

  code = raw.unpack();
  if (code != LIBRAW_SUCCESS) {
    return fail("unpack", code);
  }
  code = raw.dcraw_process();
  if (code != LIBRAW_SUCCESS) {
    return fail("dcraw_process", code);
  }
  code = raw.dcraw_ppm_tiff_writer(output.c_str());
  if (code != LIBRAW_SUCCESS) {
    return fail("dcraw_ppm_tiff_writer", code);
  }

  const auto elapsed = std::chrono::duration<double, std::milli>(
                           std::chrono::steady_clock::now() - started)
                           .count();
  std::cout << std::fixed << std::setprecision(3)
            << "{\"status\":\"passed\",\"engine\":\"LibRaw C++\","
               "\"configuration\":{\"useCameraWb\":1,"
               "\"noAutoBright\":1,\"outputColor\":1,\"outputBps\":16,"
               "\"bright\":1,\"gamma\":[0.45,4.5]},\"rawSize\":["
            << sizes.raw_width << ',' << sizes.raw_height
            << "],\"imageSize\":[" << sizes.width << ',' << sizes.height
            << "],\"make\":\"" << raw.imgdata.idata.make
            << "\",\"model\":\"" << raw.imgdata.idata.model
            << "\",\"elapsedMs\":" << elapsed << "}\n";
  return 0;
}
