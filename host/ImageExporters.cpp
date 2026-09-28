#include "ImageExporters.h"
#include <QImageWriter>
#include <QImageReader>
#include <QPainter>
#include <QFileInfo>

bool PngImageExporter::exportImage(const QImage &image, const QString &filePath, int /*quality*/) {
    if (image.isNull()) return false;
    QImageWriter writer(filePath, "PNG");
    if (!writer.write(image)) return false;

    QImageReader reader(filePath);
    QImage reimported = reader.read();
    return !reimported.isNull() && reimported.size() == image.size();
}

bool JpegImageExporter::exportImage(const QImage &image, const QString &filePath, int quality) {
    if (image.isNull()) return false;
    QImage flattened(image.size(), QImage::Format_RGB888);
    QPainter painter(&flattened);
    painter.fillRect(flattened.rect(), Qt::white);
    painter.drawImage(0, 0, image);
    painter.end();
    flattened.setDotsPerMeterX(image.dotsPerMeterX());
    flattened.setDotsPerMeterY(image.dotsPerMeterY());

    QImageWriter writer(filePath, "JPEG");
    writer.setQuality(qBound(0, quality, 100));
    return writer.write(flattened);
}

bool TiffImageExporter::exportImage(const QImage &image, const QString &filePath, int /*quality*/) {
    if (image.isNull()) return false;
    QImageWriter writer(filePath, "TIFF");
    if (!writer.write(image)) return false;

    QImageReader reader(filePath);
    QImage reimported = reader.read();
    return !reimported.isNull() && reimported.size() == image.size();
}

bool WebPImageExporter::exportImage(const QImage &image, const QString &filePath, int quality) {
    if (image.isNull()) return false;
    QImageWriter writer(filePath, "WEBP");
    writer.setQuality(qBound(0, quality, 100));
    if (!writer.write(image)) return false;

    QImageReader reader(filePath);
    QImage reimported = reader.read();
    return !reimported.isNull() && reimported.size() == image.size();
}

ImageExporterRegistry &ImageExporterRegistry::instance() {
    static ImageExporterRegistry s_instance;
    return s_instance;
}

ImageExporterRegistry::ImageExporterRegistry() {
    registerExporter(std::make_shared<PngImageExporter>());
    registerExporter(std::make_shared<JpegImageExporter>());
    registerExporter(std::make_shared<TiffImageExporter>());
    registerExporter(std::make_shared<WebPImageExporter>());
}

void ImageExporterRegistry::registerExporter(std::shared_ptr<IImageExporter> exporter) {
    if (!exporter) return;
    m_exporters.insert(QString::fromLatin1(exporter->format()).toLower(), exporter);
}

std::shared_ptr<IImageExporter> ImageExporterRegistry::exporterForFormat(const QString &format) const {
    return m_exporters.value(format.toLower(), nullptr);
}

std::shared_ptr<IImageExporter> ImageExporterRegistry::exporterForPath(const QString &filePath) const {
    const QString ext = QFileInfo(filePath).suffix().toLower();
    if (ext == "png") return exporterForFormat("png");
    if (ext == "jpg" || ext == "jpeg") return exporterForFormat("jpeg");
    if (ext == "tif" || ext == "tiff") return exporterForFormat("tiff");
    if (ext == "webp") return exporterForFormat("webp");
    return nullptr;
}
