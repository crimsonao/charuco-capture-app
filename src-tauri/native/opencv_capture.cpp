#include "opencv_capture.h"

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <vector>

#include <cmath>

#include <opencv2/calib3d.hpp>
#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>
#include <opencv2/objdetect.hpp>
#include <opencv2/videoio.hpp>

namespace {

constexpr int kBoardSquaresX = 6;
constexpr int kBoardSquaresY = 8;

struct CvCharucoImpl {
  cv::aruco::ArucoDetector detector;
  cv::aruco::CharucoDetector charuco;
};

cv::Mat frame_to_mat(const CvFrame *frame) {
  const int type = frame->channels == 1   ? CV_8UC1
                   : frame->channels == 4 ? CV_8UC4
                                          : CV_8UC3;
  return cv::Mat(frame->height, frame->width, type, frame->data);
}

int copy_mat_to_frame(const cv::Mat &mat, CvFrame *out) {
  cv::Mat continuous = mat.isContinuous() ? mat : mat.clone();
  const size_t nbytes = continuous.total() * continuous.elemSize();
  auto *data = static_cast<unsigned char *>(std::malloc(nbytes));
  if (data == nullptr) {
    return 0;
  }
  std::memcpy(data, continuous.data, nbytes);
  out->data = data;
  out->width = continuous.cols;
  out->height = continuous.rows;
  out->channels = continuous.channels();
  return 1;
}

} // namespace

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

int cvcam_imdecode_bgr(const unsigned char *data, int nbytes, CvFrame *out) {
  if (out == nullptr) {
    return 0;
  }
  out->data = nullptr;
  out->width = 0;
  out->height = 0;
  out->channels = 0;
  if (data == nullptr || nbytes <= 0) {
    return 0;
  }
  try {
    std::vector<unsigned char> buf(data, data + nbytes);
    cv::Mat decoded = cv::imdecode(buf, cv::IMREAD_COLOR);
    if (decoded.empty()) {
      return 0;
    }
    return copy_mat_to_frame(decoded, out);
  } catch (...) {
    return 0;
  }
}

double cvcam_laplacian_var(const CvFrame *frame) {
  if (frame == nullptr || frame->data == nullptr || frame->width <= 0 ||
      frame->height <= 0 || frame->channels <= 0) {
    return 0.0;
  }
  try {
    cv::Mat src = frame_to_mat(frame);
    cv::Mat gray;
    if (src.channels() == 1) {
      gray = src;
    } else if (src.channels() == 3) {
      cv::cvtColor(src, gray, cv::COLOR_BGR2GRAY);
    } else if (src.channels() == 4) {
      cv::cvtColor(src, gray, cv::COLOR_BGRA2GRAY);
    } else {
      return 0.0;
    }
    cv::Mat lap;
    cv::Laplacian(gray, lap, CV_64F);
    cv::Scalar mean;
    cv::Scalar stddev;
    cv::meanStdDev(lap, mean, stddev);
    return stddev[0] * stddev[0];
  } catch (...) {
    return 0.0;
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

CvCharuco *cvcharuco_create(float square_m, float marker_m) {
  if (square_m <= 0.0f || marker_m <= 0.0f || marker_m >= square_m) {
    return nullptr;
  }
  try {
    const cv::aruco::Dictionary dictionary =
        cv::aruco::getPredefinedDictionary(cv::aruco::DICT_4X4_50);
    const cv::aruco::CharucoBoard board(
        cv::Size(kBoardSquaresX, kBoardSquaresY), square_m, marker_m,
        dictionary);
    return reinterpret_cast<CvCharuco *>(
        new CvCharucoImpl{cv::aruco::ArucoDetector(dictionary),
                          cv::aruco::CharucoDetector(board)});
  } catch (...) {
    return nullptr;
  }
}

void cvcharuco_free(CvCharuco *board) {
  if (board == nullptr) {
    return;
  }
  delete reinterpret_cast<CvCharucoImpl *>(board);
}

int cvcharuco_detect(CvCharuco *board, const CvFrame *frame,
                     CvCharucoDetect *out) {
  if (out != nullptr) {
    out->corners = nullptr;
    out->ids = nullptr;
    out->n_corners = 0;
    out->marker_corners = nullptr;
    out->n_markers = 0;
  }
  if (board == nullptr || frame == nullptr || frame->data == nullptr ||
      out == nullptr || frame->width <= 0 || frame->height <= 0) {
    return 0;
  }
  try {
    auto *state = reinterpret_cast<CvCharucoImpl *>(board);
    cv::Mat src = frame_to_mat(frame);
    cv::Mat gray;
    if (src.channels() == 1) {
      gray = src;
    } else if (src.channels() == 3) {
      cv::cvtColor(src, gray, cv::COLOR_BGR2GRAY);
    } else if (src.channels() == 4) {
      cv::cvtColor(src, gray, cv::COLOR_BGRA2GRAY);
    } else {
      return 0;
    }

    std::vector<std::vector<cv::Point2f>> marker_corners;
    std::vector<int> marker_ids;
    state->detector.detectMarkers(gray, marker_corners, marker_ids);

    if (!marker_ids.empty()) {
      const cv::TermCriteria criteria(
          cv::TermCriteria::EPS + cv::TermCriteria::MAX_ITER, 100, 0.001);
      for (auto &corner : marker_corners) {
        cv::cornerSubPix(gray, corner, cv::Size(5, 5), cv::Size(-1, -1),
                         criteria);
      }
    }

    std::vector<cv::Point2f> charuco_corners;
    std::vector<int> charuco_ids;
    if (!marker_ids.empty()) {
      // OpenCV 4.12 dropped interpolateCornersCharuco; detectBoard with
      // pre-detected markers is the equivalent interpolation step.
      state->charuco.detectBoard(gray, charuco_corners, charuco_ids,
                                 marker_corners, marker_ids);
    }

    if (!charuco_corners.empty()) {
      const size_t n = charuco_corners.size();
      const size_t n_ids = (std::min)(n, charuco_ids.size());
      out->n_corners = static_cast<int>(n);
      out->corners =
          static_cast<float *>(std::malloc(sizeof(float) * 2 * n));
      out->ids = static_cast<int *>(std::malloc(sizeof(int) * n));
      if (out->corners == nullptr || out->ids == nullptr) {
        cvcharuco_detect_free(out);
        return 0;
      }
      for (size_t i = 0; i < n; ++i) {
        out->corners[i * 2] = charuco_corners[i].x;
        out->corners[i * 2 + 1] = charuco_corners[i].y;
        out->ids[i] = i < n_ids ? charuco_ids[i] : -1;
      }
    }

    if (!marker_corners.empty()) {
      out->n_markers = static_cast<int>(marker_corners.size());
      out->marker_corners = static_cast<float *>(
          std::malloc(sizeof(float) * 8 * marker_corners.size()));
      if (out->marker_corners == nullptr) {
        cvcharuco_detect_free(out);
        return 0;
      }
      for (size_t i = 0; i < marker_corners.size(); ++i) {
        for (int c = 0; c < 4; ++c) {
          const cv::Point2f pt =
              c < static_cast<int>(marker_corners[i].size())
                  ? marker_corners[i][static_cast<size_t>(c)]
                  : cv::Point2f();
          out->marker_corners[i * 8 + c * 2] = pt.x;
          out->marker_corners[i * 8 + c * 2 + 1] = pt.y;
        }
      }
    }
    return 1;
  } catch (...) {
    cvcharuco_detect_free(out);
    return 0;
  }
}

void cvcharuco_detect_free(CvCharucoDetect *out) {
  if (out == nullptr) {
    return;
  }
  std::free(out->corners);
  std::free(out->ids);
  std::free(out->marker_corners);
  out->corners = nullptr;
  out->ids = nullptr;
  out->n_corners = 0;
  out->marker_corners = nullptr;
  out->n_markers = 0;
}

int cvcharuco_draw(CvFrame *frame, const CvCharucoDetect *det, int enough) {
  if (frame == nullptr || frame->data == nullptr || det == nullptr ||
      frame->channels < 3) {
    return 0;
  }
  try {
    cv::Mat src = frame_to_mat(frame);
    if (det->n_markers > 0 && det->marker_corners != nullptr) {
      std::vector<std::vector<cv::Point2f>> marker_corners(
          static_cast<size_t>(det->n_markers));
      for (int i = 0; i < det->n_markers; ++i) {
        marker_corners[static_cast<size_t>(i)].resize(4);
        for (int c = 0; c < 4; ++c) {
          marker_corners[static_cast<size_t>(i)][static_cast<size_t>(c)] =
              cv::Point2f(det->marker_corners[i * 8 + c * 2],
                          det->marker_corners[i * 8 + c * 2 + 1]);
        }
      }
      cv::aruco::drawDetectedMarkers(src, marker_corners);
    }
    if (det->n_corners > 0 && det->corners != nullptr) {
      std::vector<cv::Point2f> corners(static_cast<size_t>(det->n_corners));
      for (int i = 0; i < det->n_corners; ++i) {
        corners[static_cast<size_t>(i)] =
            cv::Point2f(det->corners[i * 2], det->corners[i * 2 + 1]);
      }
      const cv::Rect box = cv::boundingRect(corners);
      const cv::Scalar color =
          enough ? cv::Scalar(0, 220, 0) : cv::Scalar(0, 200, 255);
      cv::rectangle(src, box, color, 2);
      cv::Mat corner_mat(det->n_corners, 1, CV_32FC2, det->corners);
      cv::Mat ids_mat;
      if (det->ids != nullptr) {
        ids_mat = cv::Mat(det->n_corners, 1, CV_32SC1, det->ids);
      }
      cv::aruco::drawDetectedCornersCharuco(src, corner_mat, ids_mat, color);
    }
    return 1;
  } catch (...) {
    return 0;
  }
}

void cvcharuco_calibrate_free(CvCalibResult *out) {
  if (out == nullptr) {
    return;
  }
  std::free(out->per_image);
  out->per_image = nullptr;
  out->n_images = 0;
  out->n_dist = 0;
  out->overall_rms = 0.0;
  std::memset(out->camera_matrix, 0, sizeof(out->camera_matrix));
  std::memset(out->dist_coeffs, 0, sizeof(out->dist_coeffs));
}

int cvcharuco_calibrate(CvCharuco *board, int width, int height,
                        const CvCalibView *views, int n_views,
                        CvCalibResult *out) {
  if (out != nullptr) {
    std::memset(out->camera_matrix, 0, sizeof(out->camera_matrix));
    std::memset(out->dist_coeffs, 0, sizeof(out->dist_coeffs));
    out->n_dist = 0;
    out->overall_rms = 0.0;
    out->per_image = nullptr;
    out->n_images = 0;
  }
  if (board == nullptr || views == nullptr || out == nullptr || n_views < 3 ||
      width < 16 || height < 16) {
    return 0;
  }
  try {
    auto *state = reinterpret_cast<CvCharucoImpl *>(board);
    const cv::aruco::CharucoBoard &cb = state->charuco.getBoard();
    std::vector<cv::Mat> object_points;
    std::vector<cv::Mat> image_points;
    object_points.reserve(static_cast<size_t>(n_views));
    image_points.reserve(static_cast<size_t>(n_views));

    for (int i = 0; i < n_views; ++i) {
      if (views[i].n_corners < 4 || views[i].corners == nullptr ||
          views[i].ids == nullptr) {
        return 0;
      }
      std::vector<cv::Point2f> corners(static_cast<size_t>(views[i].n_corners));
      std::vector<int> ids(static_cast<size_t>(views[i].n_corners));
      for (int j = 0; j < views[i].n_corners; ++j) {
        corners[static_cast<size_t>(j)] = cv::Point2f(
            views[i].corners[j * 2], views[i].corners[j * 2 + 1]);
        ids[static_cast<size_t>(j)] = views[i].ids[j];
      }
      cv::Mat obj;
      cv::Mat img;
      cb.matchImagePoints(corners, ids, obj, img);
      if (obj.empty() || img.empty() || obj.rows != img.rows) {
        return 0;
      }
      object_points.push_back(obj);
      image_points.push_back(img);
    }

    cv::Mat camera_matrix = cv::Mat::eye(3, 3, CV_64F);
    cv::Mat dist_coeffs = cv::Mat::zeros(5, 1, CV_64F);
    std::vector<cv::Mat> rvecs;
    std::vector<cv::Mat> tvecs;
    const double rms = cv::calibrateCamera(
        object_points, image_points, cv::Size(width, height), camera_matrix,
        dist_coeffs, rvecs, tvecs);

    for (int r = 0; r < 3; ++r) {
      for (int c = 0; c < 3; ++c) {
        out->camera_matrix[r * 3 + c] = camera_matrix.at<double>(r, c);
      }
    }
    const int n_dist =
        (std::min)(8, dist_coeffs.rows * dist_coeffs.cols);
    out->n_dist = n_dist;
    for (int i = 0; i < n_dist; ++i) {
      out->dist_coeffs[i] = dist_coeffs.at<double>(i);
    }
    out->overall_rms = rms;
    out->n_images = n_views;
    out->per_image = static_cast<double *>(
        std::malloc(sizeof(double) * static_cast<size_t>(n_views)));
    if (out->per_image == nullptr) {
      return 0;
    }

    for (int i = 0; i < n_views; ++i) {
      cv::Mat projected;
      cv::projectPoints(object_points[static_cast<size_t>(i)],
                        rvecs[static_cast<size_t>(i)],
                        tvecs[static_cast<size_t>(i)], camera_matrix,
                        dist_coeffs, projected);
      const double n = static_cast<double>(
          (std::max)(1, image_points[static_cast<size_t>(i)].rows));
      out->per_image[i] =
          cv::norm(image_points[static_cast<size_t>(i)], projected,
                   cv::NORM_L2) /
          std::sqrt(n);
    }
    return 1;
  } catch (...) {
    cvcharuco_calibrate_free(out);
    return 0;
  }
}

int cvcharuco_generate(CvCharuco *board, int width, int height, CvFrame *out) {
  if (out != nullptr) {
    out->data = nullptr;
    out->width = 0;
    out->height = 0;
    out->channels = 0;
  }
  if (board == nullptr || out == nullptr || width < 32 || height < 32) {
    return 0;
  }
  try {
    auto *state = reinterpret_cast<CvCharucoImpl *>(board);
    cv::Mat img;
    state->charuco.getBoard().generateImage(cv::Size(width, height), img, 24);
    if (img.empty()) {
      return 0;
    }
    return copy_mat_to_frame(img, out);
  } catch (...) {
    return 0;
  }
}

}
