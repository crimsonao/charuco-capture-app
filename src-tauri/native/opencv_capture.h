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

#ifdef __cplusplus
}
#endif
