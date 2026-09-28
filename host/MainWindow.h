// Main window: the Qt Widgets workspace shell (file-map counterpart of
// ContentView.swift — "Rewrite workspace layout in Qt Widgets", and CompositorApp.swift
// — "Rewrite entry point and actions in Qt"). Owns the canvas, the menu/toolbar
// actions, and the document path (open / filter / save). This is the minimal shell;
// the 28 "Rewrite Linux UI" sheet/panel counterparts are added incrementally.

#pragma once
#include "CanvasWidget.h"
#include <QMainWindow>

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(QWidget *parent = nullptr);

private slots:
    void open();
    void saveAs();
    void filterNoise();
    void filterGrain();
    void filterGradientMap();
    void filterLens();

private:
    void updateTitle();

    CanvasWidget *m_canvas;
};