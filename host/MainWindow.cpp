#include "MainWindow.h"

#include <QFileDialog>
#include <QMenuBar>
#include <QMessageBox>
#include <QStatusBar>

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent) {
    m_canvas = new CanvasWidget(this);
    setCentralWidget(m_canvas);
    resize(900, 700);

    QMenu *file = menuBar()->addMenu(tr("&File"));
    file->addAction(tr("&Open…"), QKeySequence::Open, this, &MainWindow::open);
    file->addAction(tr("Save &As…"), QKeySequence::Save, this, &MainWindow::saveAs);
    file->addSeparator();
    file->addAction(tr("&Quit"), QKeySequence::Quit, this, &QWidget::close);

    QMenu *filter = menuBar()->addMenu(tr("&Filter"));
    filter->addAction(tr("Add &Noise…"), this, &MainWindow::filterNoise);
    filter->addAction(tr("Add &Grain…"), this, &MainWindow::filterGrain);
    filter->addAction(tr("&Gradient Map…"), this, &MainWindow::filterGradientMap);
    filter->addAction(tr("&Lens Distort…"), this, &MainWindow::filterLens);

    updateTitle();
    statusBar()->showMessage(tr("Ready. Open an image to begin."));
}

void MainWindow::updateTitle() {
    setWindowTitle(m_canvas->hasImage()
                       ? tr("Compositor — %1×%2").arg(m_canvas->imageWidth()).arg(m_canvas->imageHeight())
                       : tr("Compositor"));
}

void MainWindow::open() {
    const QString path = QFileDialog::getOpenFileName(this, tr("Open Image"), QString(),
            tr("Images (*.png *.jpg *.jpeg *.bmp *.tiff *.webp);;All files (*)"));
    if (path.isEmpty()) return;
    if (!m_canvas->loadFile(path)) {
        QMessageBox::warning(this, tr("Open failed"), tr("Could not load %1").arg(path));
        return;
    }
    updateTitle();
    statusBar()->showMessage(tr("Loaded %1").arg(path), 4000);
}

void MainWindow::saveAs() {
    if (!m_canvas->hasImage()) { statusBar()->showMessage(tr("Nothing to save.")); return; }
    const QString path = QFileDialog::getSaveFileName(this, tr("Save Image"), QString(),
            tr("PNG (*.png);;JPEG (*.jpg *.jpeg);;BMP (*.bmp)"));
    if (path.isEmpty()) return;
    const char *fmt = path.endsWith(".png", Qt::CaseInsensitive) ? "PNG"
                    : path.endsWith(".bmp", Qt::CaseInsensitive) ? "BMP" : "JPEG";
    if (!m_canvas->saveFile(path, fmt))
        QMessageBox::warning(this, tr("Save failed"), tr("Could not save %1").arg(path));
    else
        statusBar()->showMessage(tr("Saved %1").arg(path), 4000);
}

void MainWindow::filterNoise() {
    if (!m_canvas->hasImage()) { statusBar()->showMessage(tr("Open an image first.")); return; }
    m_canvas->applyNoise(35.0f, 0, 0, 42);
    statusBar()->showMessage(tr("Applied noise."), 3000);
}

void MainWindow::filterGrain() {
    if (!m_canvas->hasImage()) { statusBar()->showMessage(tr("Open an image first.")); return; }
    m_canvas->applyGrain(40.0, 2.0, 50.0, 7);
    statusBar()->showMessage(tr("Applied grain."), 3000);
}

void MainWindow::filterGradientMap() {
    if (!m_canvas->hasImage()) { statusBar()->showMessage(tr("Open an image first.")); return; }
    // A simple sepia-ish gradient map: black -> dark brown, mid -> warm, white -> cream.
    uint8_t table[256 * 3];
    for (int i = 0; i < 256; ++i) {
        double t = i / 255.0;
        table[i * 3 + 0] = (uint8_t)(20 + t * 230);       // R
        table[i * 3 + 1] = (uint8_t)(10 + t * 200);       // G
        table[i * 3 + 2] = (uint8_t)(5 + t * 160);        // B
    }
    m_canvas->applyGradientMap(table);
    statusBar()->showMessage(tr("Applied gradient map."), 3000);
}

void MainWindow::filterLens() {
    if (!m_canvas->hasImage()) { statusBar()->showMessage(tr("Open an image first.")); return; }
    m_canvas->applyLensDistort(0.3);
    statusBar()->showMessage(tr("Applied lens distort."), 3000);
}