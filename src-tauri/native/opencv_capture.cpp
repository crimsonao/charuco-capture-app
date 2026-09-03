#include "opencv_capture.h"

#include <cstdlib>
#include <cstring>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/videoio.hpp>

extern "C" {

CvCam *cvcam_open(int index, int api_preference) {
  try {
    auto *cap = new cv::VideoCapture(index, api_preference);
    return reinterpret_cast<CvCam *>(cap);
  } catch (...) {
    return nullptr;
  }
}

int cvcam_is_opened(CvCam *cam) {
  if (cam == nullptr) {
    return 0;
  }
  try {
    return reinterpret_cast<cv::VideoCapture *>(cam)->isOpened() ? 1 : 0;
  } catch (...) {
    return 0;
  }
}

int cvcam_set(CvCam *cam, int prop_id, double value) {
  if (cam == nullptr) {
    return 0;
  }
  try {
    return reinterpret_cast<cv::VideoCapture *>(cam)->set(prop_id, value) ? 1
                                                                          : 0;
  } catch (...) {
    return 0;
  }
}

int cvcam_read(CvCam *cam, CvFrame *out) {
  if (cam == nullptr || out == nullptr) {
    return 0;
  }
  out->data = nullptr;
  out->width = 0;
  out->height = 0;
  out->channels = 0;
  try {
    cv::Mat frame;
    auto *cap = reinterpret_cast<cv::VideoCapture *>(cam);
    if (!cap->read(frame) || frame.empty()) {
      return 0;
    }
    if (!frame.isContinuous()) {
      frame = frame.clone();
    }
    const size_t nbytes = frame.total() * frame.elemSize();
    auto *data = static_cast<unsigned char *>(std::malloc(nbytes));
    if (data == nullptr) {
      return 0;
    }
    std::memcpy(data, frame.data, nbytes);
    out->data = data;
    out->width = frame.cols;
    out->height = frame.rows;
    out->channels = frame.channels();
    return 1;
  } catch (...) {
    return 0;
  }
}

void cvcam_frame_free(CvFrame *frame) {
  if (frame == nullptr) {
    return;
  }
  std::free(frame->data);
  frame->data = nullptr;
  frame->width = 0;
  frame->height = 0;
  frame->channels = 0;
}

double cvcam_frame_mean(const CvFrame *frame) {
  if (frame == nullptr || frame->data == nullptr || frame->width <= 0 ||
      frame->height <= 0 || frame->channels <= 0) {
    return 0.0;
  }
  const size_t count =
      static_cast<size_t>(frame->width) * static_cast<size_t>(frame->height) *
      static_cast<size_t>(frame->channels);
  double sum = 0.0;
  for (size_t i = 0; i < count; ++i) {
    sum += static_cast<double>(frame->data[i]);
  }
  return sum / static_cast<double>(count);
}

int cvcam_imencode_jpeg(const CvFrame *frame, int quality, unsigned char **out,
                        int *out_len) {
  if (out != nullptr) {
    *out = nullptr;
  }
  if (out_len != nullptr) {
    *out_len = 0;
  }
  if (frame == nullptr || frame->data == nullptr || out == nullptr ||
      out_len == nullptr) {
    return 0;
  }
  try {
    const int type = frame->channels == 1 ? CV_8UC1 : CV_8UC3;
    cv::Mat mat(frame->height, frame->width, type, frame->data);
    std::vector<int> params = {cv::IMWRITE_JPEG_QUALITY, quality};
    std::vector<unsigned char> buf;
    if (!cv::imencode(".jpg", mat, buf, params) || buf.empty()) {
      return 0;
    }
    auto *copy = static_cast<unsigned char *>(std::malloc(buf.size()));
    if (copy == nullptr) {
      return 0;
    }
    std::memcpy(copy, buf.data(), buf.size());
    *out = copy;
    *out_len = static_cast<int>(buf.size());
    return 1;
  } catch (...) {
    return 0;
  }
}

void cvcam_bytes_free(unsigned char *ptr) { std::free(ptr); }

void cvcam_release(CvCam *cam) {
  if (cam == nullptr) {
    return;
  }
  try {
    auto *cap = reinterpret_cast<cv::VideoCapture *>(cam);
    cap->release();
    delete cap;
  } catch (...) {
  }
}

}
