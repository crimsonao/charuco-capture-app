#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CvCam CvCam;

typedef struct CvFrame {
  unsigned char *data;
  int width;
  int height;
  int channels;
} CvFrame;

CvCam *cvcam_open(int index, int api_preference);
int cvcam_is_opened(CvCam *cam);
int cvcam_set(CvCam *cam, int prop_id, double value);
int cvcam_read(CvCam *cam, CvFrame *out);
void cvcam_frame_free(CvFrame *frame);
double cvcam_frame_mean(const CvFrame *frame);
int cvcam_imencode_jpeg(const CvFrame *frame, int quality, unsigned char **out,
                        int *out_len);
void cvcam_bytes_free(unsigned char *ptr);
void cvcam_release(CvCam *cam);

typedef struct CvCharuco CvCharuco;

typedef struct CvCharucoDetect {
  float *corners;
  int *ids;
  int n_corners;
  float *marker_corners;
  int n_markers;
} CvCharucoDetect;

CvCharuco *cvcharuco_create(float square_m, float marker_m);
void cvcharuco_free(CvCharuco *board);
int cvcharuco_detect(CvCharuco *board, const CvFrame *frame, CvCharucoDetect *out);
void cvcharuco_detect_free(CvCharucoDetect *out);
int cvcharuco_draw(CvFrame *frame, const CvCharucoDetect *det, int enough);
int cvcharuco_generate(CvCharuco *board, int width, int height, CvFrame *out);

#ifdef __cplusplus
}
#endif
