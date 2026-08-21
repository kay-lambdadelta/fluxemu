use nalgebra::SMatrix;

// https://en.wikipedia.org/wiki/YIQ#NTSC_1953_colorimetry
pub const YIQ_TO_RGB_NTSC_1953: SMatrix<f32, 3, 2> =
    SMatrix::<f32, 3, 2>::new(0.956, 0.619, -0.272, -0.647, -1.106, 1.703);

// https://en.wikipedia.org/wiki/Y′UV#SDTV_with_BT.470
pub const YUV_TO_RGB_SDTV_WITH_BT470: SMatrix<f32, 3, 2> =
    SMatrix::<f32, 3, 2>::new(0.0, 1.13983, -0.39465, -0.58060, 2.03211, 0.0);
